use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use arrow_array::builder::{
    BooleanBuilder, FixedSizeListBuilder, Float32Builder, Float64Builder, Int32Builder,
    Int64Builder, LargeStringBuilder, StringBuilder, UInt32Builder, UInt64Builder,
};
use arrow_array::{
    Array, ArrayRef, BooleanArray, FixedSizeListArray, Float32Array, Float64Array, Int32Array,
    Int64Array, LargeStringArray, RecordBatch, RecordBatchIterator, RecordBatchReader, StringArray,
    UInt32Array, UInt64Array,
};
use arrow_ipc::reader::StreamReader;
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures::TryStreamExt;
use lancedb::database::CreateTableMode;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::table::AddDataMode;
use lancedb::{Connection, Table};
use serde_json::{Map, Value};
use tonic::{Request, Response, Status};

use crate::config::LoggingConfig;
use crate::logging::ServiceLogger;
use crate::pb::lance_db_service_server::LanceDbService;
use crate::pb::{
    ColumnDef, ColumnType, CreateTableRequest, CreateTableResponse, DeleteRequest, DeleteResponse,
    DropTableRequest, DropTableResponse, InputFormat, OutputFormat, SearchRequest, SearchResponse,
    UpsertRequest, UpsertResponse,
};

#[derive(Clone)]
pub struct LanceDbGrpcService {
    state: Arc<ServiceState>,
}

struct ServiceState {
    db: Arc<Connection>,
    logger: Arc<ServiceLogger>,
}

#[derive(Clone, Debug)]
struct RequestLogContext {
    request_id: u64,
    operation: &'static str,
    remote_addr: String,
    summary: String,
    started_at: Instant,
    logger: Arc<ServiceLogger>,
    request_log_enabled: bool,
    slow_request_log_enabled: bool,
    slow_request_threshold: Duration,
    include_request_details_in_slow_log: bool,
}

impl LanceDbGrpcService {
    pub fn new(db: Connection, logger: Arc<ServiceLogger>) -> Self {
        Self {
            state: Arc::new(ServiceState {
                db: Arc::new(db),
                logger,
            }),
        }
    }

    async fn open_table(&self, table_name: &str) -> Result<Table, Status> {
        self.state
            .db
            .open_table(table_name.to_string())
            .execute()
            .await
            .map_err(to_status)
    }
}

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[tonic::async_trait]
impl LanceDbService for LanceDbGrpcService {
    async fn create_table(
        &self,
        request: Request<CreateTableRequest>,
    ) -> Result<Response<CreateTableResponse>, Status> {
        let context = build_request_context(
            &self.state.logger,
            "create_table",
            request.remote_addr(),
            format!(
                "table={} columns={} overwrite_if_exists={}",
                request.get_ref().table_name.trim(),
                request.get_ref().columns.len(),
                request.get_ref().overwrite_if_exists,
            ),
        );
        log_request_started(&context);
        let req = request.into_inner();

        if req.table_name.trim().is_empty() {
            let status = Status::invalid_argument("table_name must not be empty");
            log_request_failed(&context, &status);
            return Err(status);
        }
        if req.columns.is_empty() {
            let status = Status::invalid_argument("columns must not be empty");
            log_request_failed(&context, &status);
            return Err(status);
        }

        let schema = match build_arrow_schema(&req.columns) {
            Ok(schema) => schema,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };
        let mut builder = self
            .state
            .db
            .create_empty_table(req.table_name.clone(), schema);
        builder = if req.overwrite_if_exists {
            builder.mode(CreateTableMode::Overwrite)
        } else {
            builder.mode(CreateTableMode::Create)
        };

        match builder.execute().await.map_err(to_status) {
            Ok(_) => {}
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        }

        let response = CreateTableResponse {
            success: true,
            message: format!("table '{}' is ready", req.table_name),
        };
        log_request_succeeded(&context, response.message.as_str());
        Ok(Response::new(response))
    }

    async fn vector_upsert(
        &self,
        request: Request<UpsertRequest>,
    ) -> Result<Response<UpsertResponse>, Status> {
        let context = build_request_context(
            &self.state.logger,
            "vector_upsert",
            request.remote_addr(),
            format!(
                "table={} key_columns={} input_format={:?} payload_bytes={}",
                request.get_ref().table_name.trim(),
                request.get_ref().key_columns.len(),
                request.get_ref().input_format(),
                request.get_ref().data.len(),
            ),
        );
        log_request_started(&context);
        let req = request.into_inner();

        if req.table_name.trim().is_empty() {
            let status = Status::invalid_argument("table_name must not be empty");
            log_request_failed(&context, &status);
            return Err(status);
        }

        let table = match self.open_table(&req.table_name).await {
            Ok(table) => table,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };
        let schema = match table.schema().await.map_err(to_status) {
            Ok(schema) => schema,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };
        let (batches, input_rows) =
            match decode_input_to_batches(req.input_format(), &req.data, schema) {
                Ok(result) => result,
                Err(status) => {
                    log_request_failed(&context, &status);
                    return Err(status);
                }
            };

        if input_rows == 0 {
            let version = match table.version().await.map_err(to_status) {
                Ok(version) => version,
                Err(status) => {
                    log_request_failed(&context, &status);
                    return Err(status);
                }
            };
            let response = UpsertResponse {
                success: true,
                message: "no rows to write".to_string(),
                version,
                input_rows: 0,
                inserted_rows: 0,
                updated_rows: 0,
                deleted_rows: 0,
            };
            log_request_succeeded(&context, response.message.as_str());
            return Ok(Response::new(response));
        }

        let schema = match table.schema().await.map_err(to_status) {
            Ok(schema) => schema,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };
        let reader: Box<dyn RecordBatchReader + Send> = Box::new(RecordBatchIterator::new(
            batches.into_iter().map(Ok),
            schema.clone(),
        ));

        let response = if req.key_columns.is_empty() {
            match table
                .add(reader)
                .mode(AddDataMode::Append)
                .execute()
                .await
                .map_err(to_status)
            {
                Ok(_) => {}
                Err(status) => {
                    log_request_failed(&context, &status);
                    return Err(status);
                }
            }

            UpsertResponse {
                success: true,
                message: "append completed".to_string(),
                version: match table.version().await.map_err(to_status) {
                    Ok(version) => version,
                    Err(status) => {
                        log_request_failed(&context, &status);
                        return Err(status);
                    }
                },
                input_rows,
                inserted_rows: input_rows,
                updated_rows: 0,
                deleted_rows: 0,
            }
        } else {
            let keys = req
                .key_columns
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();

            let mut merge = table.merge_insert(&keys);
            merge
                .when_matched_update_all(None)
                .when_not_matched_insert_all();

            let result = match merge.execute(reader).await.map_err(to_status) {
                Ok(result) => result,
                Err(status) => {
                    log_request_failed(&context, &status);
                    return Err(status);
                }
            };

            UpsertResponse {
                success: true,
                message: "merge upsert completed".to_string(),
                version: result.version,
                input_rows,
                inserted_rows: result.num_inserted_rows,
                updated_rows: result.num_updated_rows,
                deleted_rows: result.num_deleted_rows,
            }
        };

        log_request_succeeded(&context, response.message.as_str());
        Ok(Response::new(response))
    }

    async fn vector_search(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<SearchResponse>, Status> {
        let context = build_request_context(
            &self.state.logger,
            "vector_search",
            request.remote_addr(),
            format!(
                "table={} vector_dim={} limit={} output_format={:?} filter=\"{}\"",
                request.get_ref().table_name.trim(),
                request.get_ref().vector.len(),
                request.get_ref().limit,
                request.get_ref().output_format(),
                preview_text(
                    request.get_ref().filter.trim(),
                    self.state.logger.config().request_preview_chars
                ),
            ),
        );
        log_request_started(&context);
        let req = request.into_inner();

        if req.table_name.trim().is_empty() {
            let status = Status::invalid_argument("table_name must not be empty");
            log_request_failed(&context, &status);
            return Err(status);
        }
        if req.vector.is_empty() {
            let status = Status::invalid_argument("vector must not be empty");
            log_request_failed(&context, &status);
            return Err(status);
        }

        let table = match self.open_table(&req.table_name).await {
            Ok(table) => table,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };
        let output_format = req.output_format();
        let vector = req.vector;
        let mut query = match table.query().nearest_to(vector).map_err(to_status) {
            Ok(query) => query,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };

        if !req.vector_column.trim().is_empty() {
            query = query.column(req.vector_column.trim());
        }

        let limit = if req.limit == 0 {
            10
        } else {
            req.limit as usize
        };
        query = query.limit(limit);

        if !req.filter.trim().is_empty() {
            query = query.only_if(req.filter.trim());
        }

        let output_schema = match query.output_schema().await.map_err(to_status) {
            Ok(schema) => schema,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };
        let stream = match query.execute().await.map_err(to_status) {
            Ok(stream) => stream,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };
        let batches: Vec<RecordBatch> = match stream.try_collect().await.map_err(to_status) {
            Ok(batches) => batches,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };
        let rows = count_rows(&batches);

        let (format, data) = match output_format {
            OutputFormat::JsonRows => (
                "json".to_string(),
                match encode_batches_as_json(&output_schema, &batches) {
                    Ok(data) => data,
                    Err(status) => {
                        log_request_failed(&context, &status);
                        return Err(status);
                    }
                },
            ),
            OutputFormat::Unspecified | OutputFormat::ArrowIpc => (
                "arrow_ipc".to_string(),
                match encode_batches_as_arrow_ipc(&output_schema, &batches) {
                    Ok(data) => data,
                    Err(status) => {
                        log_request_failed(&context, &status);
                        return Err(status);
                    }
                },
            ),
        };

        let response = SearchResponse {
            success: true,
            message: "search completed".to_string(),
            format,
            rows,
            data,
        };
        log_request_succeeded(
            &context,
            format!("{} rows encoded as {}", response.rows, response.format),
        );
        Ok(Response::new(response))
    }

    async fn delete(
        &self,
        request: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let context = build_request_context(
            &self.state.logger,
            "delete",
            request.remote_addr(),
            format!(
                "table={} condition=\"{}\"",
                request.get_ref().table_name.trim(),
                preview_text(
                    request.get_ref().condition.trim(),
                    self.state.logger.config().request_preview_chars
                ),
            ),
        );
        log_request_started(&context);
        let req = request.into_inner();

        if req.table_name.trim().is_empty() {
            let status = Status::invalid_argument("table_name must not be empty");
            log_request_failed(&context, &status);
            return Err(status);
        }
        if req.condition.trim().is_empty() {
            let status = Status::invalid_argument("condition must not be empty");
            log_request_failed(&context, &status);
            return Err(status);
        }

        let table = match self.open_table(&req.table_name).await {
            Ok(table) => table,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };
        let result = match table.delete(req.condition.trim()).await.map_err(to_status) {
            Ok(result) => result,
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        };

        let response = DeleteResponse {
            success: true,
            message: format!("delete completed for '{}'", req.table_name),
            version: result.version,
            deleted_rows: result.num_deleted_rows,
        };
        log_request_succeeded(&context, format!("deleted_rows={}", response.deleted_rows));
        Ok(Response::new(response))
    }

    async fn drop_table(
        &self,
        request: Request<DropTableRequest>,
    ) -> Result<Response<DropTableResponse>, Status> {
        let context = build_request_context(
            &self.state.logger,
            "drop_table",
            request.remote_addr(),
            format!("table={}", request.get_ref().table_name.trim()),
        );
        log_request_started(&context);
        let req = request.into_inner();

        if req.table_name.trim().is_empty() {
            let status = Status::invalid_argument("table_name must not be empty");
            log_request_failed(&context, &status);
            return Err(status);
        }

        match self
            .state
            .db
            .drop_table(req.table_name.clone(), &[])
            .await
            .map_err(to_status)
        {
            Ok(_) => {}
            Err(status) => {
                log_request_failed(&context, &status);
                return Err(status);
            }
        }

        let response = DropTableResponse {
            success: true,
            message: format!("table '{}' dropped", req.table_name),
        };
        log_request_succeeded(&context, response.message.as_str());
        Ok(Response::new(response))
    }
}

fn build_request_context(
    logger: &Arc<ServiceLogger>,
    operation: &'static str,
    remote_addr: Option<std::net::SocketAddr>,
    summary: String,
) -> RequestLogContext {
    let logging: &LoggingConfig = logger.config();
    RequestLogContext {
        request_id: NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed),
        operation,
        remote_addr: remote_addr
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        summary,
        started_at: Instant::now(),
        logger: Arc::clone(logger),
        request_log_enabled: logging.request_log_enabled,
        slow_request_log_enabled: logging.slow_request_log_enabled,
        slow_request_threshold: Duration::from_millis(logging.slow_request_threshold_ms),
        include_request_details_in_slow_log: logging.include_request_details_in_slow_log,
    }
}

fn log_request_started(context: &RequestLogContext) {
    if !context.request_log_enabled {
        return;
    }

    context.logger.log(
        "start",
        format!(
            "request_id={} op={} remote={} summary={}",
            context.request_id, context.operation, context.remote_addr, context.summary
        ),
    );
}

fn log_request_succeeded(context: &RequestLogContext, detail: impl AsRef<str>) {
    let elapsed = context.started_at.elapsed();
    if context.request_log_enabled {
        context.logger.log(
            "ok",
            format!(
                "request_id={} op={} elapsed_ms={} remote={} detail={} summary={}",
                context.request_id,
                context.operation,
                elapsed.as_millis(),
                context.remote_addr,
                detail.as_ref(),
                context.summary,
            ),
        );
    }
    maybe_log_slow_request(context, elapsed, "completed", detail.as_ref());
}

fn log_request_failed(context: &RequestLogContext, status: &Status) {
    let elapsed = context.started_at.elapsed();
    context.logger.log(
        "error",
        format!(
            "request_id={} op={} elapsed_ms={} remote={} code={:?} message={} summary={}",
            context.request_id,
            context.operation,
            elapsed.as_millis(),
            context.remote_addr,
            status.code(),
            status.message(),
            context.summary,
        ),
    );
    maybe_log_slow_request(context, elapsed, "failed", status.message());
}

fn maybe_log_slow_request(
    context: &RequestLogContext,
    elapsed: Duration,
    final_state: &str,
    detail: &str,
) {
    if !context.slow_request_log_enabled || elapsed < context.slow_request_threshold {
        return;
    }

    let summary = if context.include_request_details_in_slow_log {
        context.summary.as_str()
    } else {
        context.operation
    };

    context.logger.log(
        "slow_request",
        format!(
            "request_id={} op={} elapsed_ms={} threshold_ms={} remote={} state={} detail={} summary={}",
            context.request_id,
            context.operation,
            elapsed.as_millis(),
            context.slow_request_threshold.as_millis(),
            context.remote_addr,
            final_state,
            detail,
            summary,
        ),
    );
}

fn preview_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "<empty>".to_string();
    }

    let mut preview = String::new();
    for (index, ch) in normalized.chars().enumerate() {
        if index >= max_chars {
            preview.push_str("...");
            return preview;
        }
        preview.push(ch);
    }

    preview
}

fn build_arrow_schema(columns: &[ColumnDef]) -> Result<SchemaRef, Status> {
    let mut fields = Vec::with_capacity(columns.len());
    for column in columns {
        if column.name.trim().is_empty() {
            return Err(Status::invalid_argument("column name must not be empty"));
        }
        fields.push(column_to_field(column)?);
    }
    Ok(Arc::new(Schema::new(fields)))
}

fn column_to_field(column: &ColumnDef) -> Result<Field, Status> {
    let data_type = match column.column_type() {
        ColumnType::String => DataType::Utf8,
        ColumnType::Int64 => DataType::Int64,
        ColumnType::Float64 => DataType::Float64,
        ColumnType::Bool => DataType::Boolean,
        ColumnType::Float32 => DataType::Float32,
        ColumnType::Uint64 => DataType::UInt64,
        ColumnType::Int32 => DataType::Int32,
        ColumnType::Uint32 => DataType::UInt32,
        ColumnType::VectorFloat32 => {
            if column.vector_dim == 0 {
                return Err(Status::invalid_argument(format!(
                    "vector column '{}' must have vector_dim > 0",
                    column.name
                )));
            }
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                column.vector_dim as i32,
            )
        }
        ColumnType::Unspecified => {
            return Err(Status::invalid_argument(format!(
                "column '{}' has unspecified type",
                column.name
            )));
        }
    };

    Ok(Field::new(&column.name, data_type, column.nullable))
}

fn decode_input_to_batches(
    format: InputFormat,
    data: &[u8],
    schema: SchemaRef,
) -> Result<(Vec<RecordBatch>, u64), Status> {
    match format {
        InputFormat::JsonRows | InputFormat::Unspecified => {
            decode_json_rows_to_batches(data, schema)
        }
        InputFormat::ArrowIpc => decode_arrow_ipc_to_batches(data),
    }
}

fn decode_arrow_ipc_to_batches(data: &[u8]) -> Result<(Vec<RecordBatch>, u64), Status> {
    let mut reader = StreamReader::try_new(Cursor::new(data.to_vec()), None).map_err(to_status)?;
    let mut batches = Vec::new();
    let mut rows = 0_u64;

    for batch in &mut reader {
        let batch = batch.map_err(to_status)?;
        rows += batch.num_rows() as u64;
        batches.push(batch);
    }

    Ok((batches, rows))
}

fn decode_json_rows_to_batches(
    data: &[u8],
    schema: SchemaRef,
) -> Result<(Vec<RecordBatch>, u64), Status> {
    let rows: Vec<Value> = if data.is_empty() {
        Vec::new()
    } else {
        serde_json::from_slice(data).map_err(|e| {
            Status::invalid_argument(format!(
                "failed to parse JSON rows, expected a JSON array of objects: {e}"
            ))
        })?
    };

    let batch = json_rows_to_record_batch(&rows, schema)?;
    let row_count = batch.num_rows() as u64;
    Ok((vec![batch], row_count))
}

fn json_rows_to_record_batch(rows: &[Value], schema: SchemaRef) -> Result<RecordBatch, Status> {
    let mut arrays = Vec::<ArrayRef>::with_capacity(schema.fields().len());

    for field in schema.fields() {
        arrays.push(build_array_for_field(
            rows,
            field.name(),
            field.data_type(),
            field.is_nullable(),
        )?);
    }

    RecordBatch::try_new(schema, arrays).map_err(to_status)
}

fn build_array_for_field(
    rows: &[Value],
    field_name: &str,
    data_type: &DataType,
    nullable: bool,
) -> Result<ArrayRef, Status> {
    match data_type {
        DataType::Utf8 => {
            let mut builder = StringBuilder::new();
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value(expect_string(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::LargeUtf8 => {
            let mut builder = LargeStringBuilder::new();
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value(expect_string(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Int64 => {
            let mut builder = Int64Builder::with_capacity(rows.len());
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value(expect_i64(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Int32 => {
            let mut builder = Int32Builder::with_capacity(rows.len());
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value(expect_i32(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::UInt64 => {
            let mut builder = UInt64Builder::with_capacity(rows.len());
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value(expect_u64(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::UInt32 => {
            let mut builder = UInt32Builder::with_capacity(rows.len());
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value(expect_u32(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Float64 => {
            let mut builder = Float64Builder::with_capacity(rows.len());
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value(expect_f64(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Float32 => {
            let mut builder = Float32Builder::with_capacity(rows.len());
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value(expect_f32(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Boolean => {
            let mut builder = BooleanBuilder::with_capacity(rows.len());
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value(expect_bool(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::FixedSizeList(child, dim) if child.data_type() == &DataType::Float32 => {
            let mut builder = FixedSizeListBuilder::with_capacity(
                Float32Builder::with_capacity(rows.len() * (*dim as usize)),
                *dim,
                rows.len(),
            );

            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => {
                        let array = value.as_array().ok_or_else(|| {
                            Status::invalid_argument(format!(
                                "field '{}' must be a JSON array of float32 values",
                                field_name
                            ))
                        })?;
                        if array.len() != *dim as usize {
                            return Err(Status::invalid_argument(format!(
                                "field '{}' length mismatch: expected {}, got {}",
                                field_name,
                                dim,
                                array.len()
                            )));
                        }
                        for item in array {
                            builder.values().append_value(expect_f32(item, field_name)?);
                        }
                        builder.append(true);
                    }
                    None => {
                        for _ in 0..*dim {
                            builder.values().append_null();
                        }
                        builder.append(false);
                    }
                }
            }

            Ok(Arc::new(builder.finish()))
        }
        other => Err(Status::invalid_argument(format!(
            "unsupported field type for JSON ingestion on '{}': {:?}",
            field_name, other
        ))),
    }
}

fn extract_field_value<'a>(
    row: &'a Value,
    field_name: &str,
    nullable: bool,
) -> Result<Option<&'a Value>, Status> {
    let object = row.as_object().ok_or_else(|| {
        Status::invalid_argument("JSON rows must be an array of JSON objects".to_string())
    })?;

    match object.get(field_name) {
        Some(Value::Null) => {
            if nullable {
                Ok(None)
            } else {
                Err(Status::invalid_argument(format!(
                    "field '{}' is not nullable",
                    field_name
                )))
            }
        }
        Some(value) => Ok(Some(value)),
        None => {
            if nullable {
                Ok(None)
            } else {
                Err(Status::invalid_argument(format!(
                    "field '{}' is missing and not nullable",
                    field_name
                )))
            }
        }
    }
}

fn expect_string<'a>(value: &'a Value, field_name: &str) -> Result<&'a str, Status> {
    value
        .as_str()
        .ok_or_else(|| Status::invalid_argument(format!("field '{}' must be a string", field_name)))
}

fn expect_i64(value: &Value, field_name: &str) -> Result<i64, Status> {
    value
        .as_i64()
        .ok_or_else(|| Status::invalid_argument(format!("field '{}' must be an int64", field_name)))
}

fn expect_i32(value: &Value, field_name: &str) -> Result<i32, Status> {
    let raw = value.as_i64().ok_or_else(|| {
        Status::invalid_argument(format!("field '{}' must be an int32", field_name))
    })?;
    i32::try_from(raw).map_err(|_| {
        Status::invalid_argument(format!("field '{}' is out of int32 range", field_name))
    })
}

fn expect_u64(value: &Value, field_name: &str) -> Result<u64, Status> {
    value
        .as_u64()
        .ok_or_else(|| Status::invalid_argument(format!("field '{}' must be a uint64", field_name)))
}

fn expect_u32(value: &Value, field_name: &str) -> Result<u32, Status> {
    let raw = value.as_u64().ok_or_else(|| {
        Status::invalid_argument(format!("field '{}' must be a uint32", field_name))
    })?;
    u32::try_from(raw).map_err(|_| {
        Status::invalid_argument(format!("field '{}' is out of uint32 range", field_name))
    })
}

fn expect_f64(value: &Value, field_name: &str) -> Result<f64, Status> {
    value.as_f64().ok_or_else(|| {
        Status::invalid_argument(format!("field '{}' must be a float64", field_name))
    })
}

fn expect_f32(value: &Value, field_name: &str) -> Result<f32, Status> {
    let raw = value.as_f64().ok_or_else(|| {
        Status::invalid_argument(format!("field '{}' must be a float32", field_name))
    })?;
    Ok(raw as f32)
}

fn expect_bool(value: &Value, field_name: &str) -> Result<bool, Status> {
    value
        .as_bool()
        .ok_or_else(|| Status::invalid_argument(format!("field '{}' must be a bool", field_name)))
}

fn encode_batches_as_arrow_ipc(
    schema: &SchemaRef,
    batches: &[RecordBatch],
) -> Result<Vec<u8>, Status> {
    let mut writer = StreamWriter::try_new(Vec::<u8>::new(), schema.as_ref()).map_err(to_status)?;
    for batch in batches {
        writer.write(batch).map_err(to_status)?;
    }
    writer.finish().map_err(to_status)?;
    writer.into_inner().map_err(to_status)
}

fn encode_batches_as_json(schema: &SchemaRef, batches: &[RecordBatch]) -> Result<Vec<u8>, Status> {
    let mut rows = Vec::<Value>::new();
    for batch in batches {
        for row_idx in 0..batch.num_rows() {
            let mut object = Map::<String, Value>::new();
            for (col_idx, field) in schema.fields().iter().enumerate() {
                let value = json_value_from_array(batch.column(col_idx), row_idx)?;
                object.insert(field.name().clone(), value);
            }
            rows.push(Value::Object(object));
        }
    }

    serde_json::to_vec(&rows).map_err(to_status)
}

fn json_value_from_array(array: &ArrayRef, row_idx: usize) -> Result<Value, Status> {
    if array.is_null(row_idx) {
        return Ok(Value::Null);
    }

    match array.data_type() {
        DataType::Utf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| Status::internal("failed to downcast Utf8 array".to_string()))?;
            Ok(Value::String(arr.value(row_idx).to_string()))
        }
        DataType::LargeUtf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .ok_or_else(|| {
                    Status::internal("failed to downcast LargeUtf8 array".to_string())
                })?;
            Ok(Value::String(arr.value(row_idx).to_string()))
        }
        DataType::Int64 => {
            let arr = array
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or_else(|| Status::internal("failed to downcast Int64 array".to_string()))?;
            Ok(Value::from(arr.value(row_idx)))
        }
        DataType::Int32 => {
            let arr = array
                .as_any()
                .downcast_ref::<Int32Array>()
                .ok_or_else(|| Status::internal("failed to downcast Int32 array".to_string()))?;
            Ok(Value::from(arr.value(row_idx)))
        }
        DataType::UInt64 => {
            let arr = array
                .as_any()
                .downcast_ref::<UInt64Array>()
                .ok_or_else(|| Status::internal("failed to downcast UInt64 array".to_string()))?;
            Ok(Value::from(arr.value(row_idx)))
        }
        DataType::UInt32 => {
            let arr = array
                .as_any()
                .downcast_ref::<UInt32Array>()
                .ok_or_else(|| Status::internal("failed to downcast UInt32 array".to_string()))?;
            Ok(Value::from(arr.value(row_idx)))
        }
        DataType::Float64 => {
            let arr = array
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| Status::internal("failed to downcast Float64 array".to_string()))?;
            Ok(Value::from(arr.value(row_idx)))
        }
        DataType::Float32 => {
            let arr = array
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or_else(|| Status::internal("failed to downcast Float32 array".to_string()))?;
            Ok(Value::from(arr.value(row_idx) as f64))
        }
        DataType::Boolean => {
            let arr = array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or_else(|| Status::internal("failed to downcast Boolean array".to_string()))?;
            Ok(Value::from(arr.value(row_idx)))
        }
        DataType::FixedSizeList(child, _) if child.data_type() == &DataType::Float32 => {
            let arr = array
                .as_any()
                .downcast_ref::<FixedSizeListArray>()
                .ok_or_else(|| {
                    Status::internal("failed to downcast FixedSizeList array".to_string())
                })?;
            let values = arr.value(row_idx);
            let floats = values
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or_else(|| {
                    Status::internal(
                        "failed to downcast FixedSizeList child Float32 array".to_string(),
                    )
                })?;

            let mut items = Vec::with_capacity(floats.len());
            for idx in 0..floats.len() {
                if floats.is_null(idx) {
                    items.push(Value::Null);
                } else {
                    items.push(Value::from(floats.value(idx) as f64));
                }
            }
            Ok(Value::Array(items))
        }
        other => Err(Status::internal(format!(
            "unsupported output type for JSON encoding: {:?}",
            other
        ))),
    }
}

fn count_rows(batches: &[RecordBatch]) -> u64 {
    batches.iter().map(|b| b.num_rows() as u64).sum()
}

fn to_status<E: std::fmt::Display>(error: E) -> Status {
    Status::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::preview_text;

    #[test]
    fn preview_text_compacts_whitespace_and_truncates() {
        let preview = preview_text("table = demo\nfilter = id   > 1", 160);
        assert_eq!(preview, "table = demo filter = id > 1");

        let preview = preview_text(&format!("prefix {}", "x".repeat(300)), 24);
        assert!(preview.ends_with("..."));
        assert!(preview.len() >= 24);
    }

    #[test]
    fn preview_text_marks_empty_input() {
        assert_eq!(preview_text(" \n\t ", 64), "<empty>");
    }
}
