use crate::config::Config;
use crate::pb::duck_db_service_server::DuckDbService;
use crate::pb::{ExecuteRequest, ExecuteResponse, QueryJsonResponse, QueryRequest, QueryResponse};
use arrow::ipc::writer::StreamWriter;
use bytes::Bytes;
use duckdb::types::{
    TimeUnit as DuckTimeUnit, ToSql, Value as DuckValue, ValueRef as DuckValueRef,
};
use duckdb::Connection;
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::io;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

const STREAM_CHANNEL_CAPACITY: usize = 8;
const DEFAULT_IPC_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
struct AppState {
    root_connection: Arc<Mutex<Connection>>,
    config: Config,
}

#[derive(Clone, Debug)]
pub struct DuckDbGrpcService {
    state: Arc<AppState>,
}

impl DuckDbGrpcService {
    pub fn new(root_connection: Connection, config: Config) -> Self {
        Self {
            state: Arc::new(AppState {
                root_connection: Arc::new(Mutex::new(root_connection)),
                config,
            }),
        }
    }
}

#[tonic::async_trait]
impl DuckDbService for DuckDbGrpcService {
    async fn execute_script(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<ExecuteResponse>, Status> {
        let req = request.into_inner();
        let sql = req.sql;
        if sql.trim().is_empty() {
            return Err(Status::invalid_argument("sql must not be empty"));
        }

        let state = Arc::clone(&self.state);
        let response =
            tokio::task::spawn_blocking(move || run_execute_script(state, sql, req.params_json))
                .await
                .map_err(|err| Status::internal(format!("execute worker join failed: {err}")))??;

        Ok(Response::new(response))
    }

    type QueryStreamStream = ReceiverStream<Result<QueryResponse, Status>>;

    async fn query_stream(
        &self,
        request: Request<QueryRequest>,
    ) -> Result<Response<Self::QueryStreamStream>, Status> {
        let req = request.into_inner();
        let sql = req.sql;
        if sql.trim().is_empty() {
            return Err(Status::invalid_argument("sql must not be empty"));
        }

        let (tx, rx) = mpsc::channel(STREAM_CHANNEL_CAPACITY);
        let worker_tx = tx.clone();
        let join_tx = tx.clone();
        let state = Arc::clone(&self.state);

        let worker = tokio::task::spawn_blocking(move || {
            run_query_streaming(state, sql, req.params_json, worker_tx)
        });

        tokio::spawn(async move {
            match worker.await {
                Ok(Ok(())) => {}
                Ok(Err(status)) => {
                    let _ = join_tx.send(Err(status)).await;
                }
                Err(err) => {
                    let _ = join_tx
                        .send(Err(Status::internal(format!(
                            "query worker join failed: {err}"
                        ))))
                        .await;
                }
            }
        });

        drop(tx);

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn query_json(
        &self,
        request: Request<QueryRequest>,
    ) -> Result<Response<QueryJsonResponse>, Status> {
        let req = request.into_inner();
        if req.sql.trim().is_empty() {
            return Err(Status::invalid_argument("sql must not be empty"));
        }

        let state = Arc::clone(&self.state);
        let response =
            tokio::task::spawn_blocking(move || run_query_json(state, req.sql, req.params_json))
                .await
                .map_err(|err| {
                    Status::internal(format!("query_json worker join failed: {err}"))
                })??;

        Ok(Response::new(response))
    }
}

pub fn apply_connection_pragmas(conn: &Connection, config: &Config) -> duckdb::Result<()> {
    let escaped_memory_limit = config.memory_limit.replace('\'', "''");
    let sql = format!(
        "PRAGMA memory_limit='{}'; PRAGMA threads={};",
        escaped_memory_limit, config.threads
    );
    conn.execute_batch(&sql)
}

fn clone_configured_connection(state: &Arc<AppState>) -> Result<Connection, Status> {
    let cloned = {
        let guard = state
            .root_connection
            .lock()
            .map_err(|_| Status::internal("duckdb root connection mutex is poisoned"))?;

        guard
            .try_clone()
            .map_err(|err| Status::internal(format!("duckdb connection clone failed: {err}")))?
    };

    apply_connection_pragmas(&cloned, &state.config)
        .map_err(|err| Status::internal(format!("duckdb pragma setup failed: {err}")))?;

    Ok(cloned)
}

fn run_execute_script(
    state: Arc<AppState>,
    sql: String,
    params_json: String,
) -> Result<ExecuteResponse, Status> {
    let conn = clone_configured_connection(&state)?;
    let bound_values = parse_bound_params(&params_json)?;

    if bound_values.is_empty() {
        conn.execute_batch(&sql)
            .map_err(|err| status_internal("duckdb execute_batch failed", err))?;

        return Ok(ExecuteResponse {
            success: true,
            message: "script executed successfully".to_string(),
        });
    }

    if has_multiple_sql_statements(&sql) {
        return Err(Status::invalid_argument(
            "params_json is only supported for a single SQL statement",
        ));
    }

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|err| status_internal("duckdb prepare failed", err))?;
    let params = bind_values_as_params(&bound_values);
    let rows_changed = stmt
        .execute(params.as_slice())
        .map_err(|err| status_internal("duckdb execute failed", err))?;

    Ok(ExecuteResponse {
        success: true,
        message: format!("statement executed successfully (rows_changed={rows_changed})"),
    })
}

fn run_query_streaming(
    state: Arc<AppState>,
    sql: String,
    params_json: String,
    tx: mpsc::Sender<Result<QueryResponse, Status>>,
) -> Result<(), Status> {
    let conn = clone_configured_connection(&state)?;
    let bound_values = parse_bound_params(&params_json)?;
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|err| status_internal("duckdb prepare failed", err))?;
    let params = bind_values_as_params(&bound_values);

    let mut batches = stmt
        .query_arrow(params.as_slice())
        .map_err(|err| status_internal("duckdb query_arrow failed", err))?;

    let schema = batches.get_schema();
    let chunk_writer = GrpcChunkWriter::new(tx, DEFAULT_IPC_CHUNK_BYTES);
    let mut ipc_writer = StreamWriter::try_new(chunk_writer, &schema)
        .map_err(|err| status_internal("arrow stream header write failed", err))?;

    ipc_writer
        .flush()
        .map_err(|err| status_internal("arrow stream flush failed", err))?;

    for batch in &mut batches {
        ipc_writer
            .write(&batch)
            .map_err(|err| status_internal("arrow batch write failed", err))?;

        ipc_writer
            .flush()
            .map_err(|err| status_internal("arrow batch flush failed", err))?;
    }

    ipc_writer
        .finish()
        .map_err(|err| status_internal("arrow stream finish failed", err))?;

    ipc_writer
        .flush()
        .map_err(|err| status_internal("arrow final flush failed", err))?;

    Ok(())
}

fn run_query_json(
    state: Arc<AppState>,
    sql: String,
    params_json: String,
) -> Result<QueryJsonResponse, Status> {
    let conn = clone_configured_connection(&state)?;
    let bound_values = parse_bound_params(&params_json)?;
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|err| status_internal("duckdb prepare failed", err))?;
    let params = bind_values_as_params(&bound_values);
    let mut rows = stmt
        .query(params.as_slice())
        .map_err(|err| status_internal("duckdb query failed", err))?;
    let column_names = rows
        .as_ref()
        .ok_or_else(|| Status::internal("duckdb rows lost statement metadata"))?
        .column_names();

    let mut json_rows = Vec::<JsonValue>::new();
    while let Some(row) = rows
        .next()
        .map_err(|err| status_internal("duckdb row fetch failed", err))?
    {
        let mut object = JsonMap::new();
        for (index, column_name) in column_names.iter().enumerate() {
            let value = row
                .get_ref(index)
                .map_err(|err| status_internal("duckdb value access failed", err))?;
            object.insert(column_name.clone(), duckdb_value_ref_to_json(value));
        }
        json_rows.push(JsonValue::Object(object));
    }

    let json_data = serde_json::to_string(&json_rows)
        .map_err(|err| status_internal("serialize JSON result failed", err))?;

    Ok(QueryJsonResponse { json_data })
}

fn parse_bound_params(params_json: &str) -> Result<Vec<DuckValue>, Status> {
    if params_json.trim().is_empty() {
        return Ok(Vec::new());
    }

    let params = serde_json::from_str::<JsonValue>(params_json).map_err(|err| {
        Status::invalid_argument(format!(
            "params_json must be a JSON array of scalar values: {err}"
        ))
    })?;

    let items = params.as_array().ok_or_else(|| {
        Status::invalid_argument("params_json must be a JSON array of scalar values")
    })?;

    items
        .iter()
        .cloned()
        .map(json_param_to_duck_value)
        .collect()
}

fn json_param_to_duck_value(value: JsonValue) -> Result<DuckValue, Status> {
    match value {
        JsonValue::Null => Ok(DuckValue::Null),
        JsonValue::Bool(value) => Ok(DuckValue::Boolean(value)),
        JsonValue::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(DuckValue::BigInt(value))
            } else if let Some(value) = value.as_u64() {
                Ok(DuckValue::UBigInt(value))
            } else if let Some(value) = value.as_f64() {
                Ok(DuckValue::Double(value))
            } else {
                Err(Status::invalid_argument(
                    "params_json contains an unsupported numeric value",
                ))
            }
        }
        JsonValue::String(value) => Ok(DuckValue::Text(value)),
        JsonValue::Array(_) | JsonValue::Object(_) => Err(Status::invalid_argument(
            "params_json only supports scalar JSON values (null, bool, number, string)",
        )),
    }
}

fn bind_values_as_params(values: &[DuckValue]) -> Vec<&dyn ToSql> {
    values.iter().map(|value| value as &dyn ToSql).collect()
}

fn has_multiple_sql_statements(sql: &str) -> bool {
    sql.split(';')
        .filter(|segment| !segment.trim().is_empty())
        .count()
        > 1
}

fn duckdb_value_ref_to_json(value: DuckValueRef<'_>) -> JsonValue {
    duckdb_value_to_json(&value.to_owned())
}

fn duckdb_value_to_json(value: &DuckValue) -> JsonValue {
    match value {
        DuckValue::Null => JsonValue::Null,
        DuckValue::Boolean(value) => JsonValue::Bool(*value),
        DuckValue::TinyInt(value) => JsonValue::from(*value),
        DuckValue::SmallInt(value) => JsonValue::from(*value),
        DuckValue::Int(value) => JsonValue::from(*value),
        DuckValue::BigInt(value) => JsonValue::from(*value),
        DuckValue::HugeInt(value) => JsonValue::String(value.to_string()),
        DuckValue::UTinyInt(value) => JsonValue::from(*value),
        DuckValue::USmallInt(value) => JsonValue::from(*value),
        DuckValue::UInt(value) => JsonValue::from(*value),
        DuckValue::UBigInt(value) => JsonValue::from(*value),
        DuckValue::Float(value) => json_float(f64::from(*value)),
        DuckValue::Double(value) => json_float(*value),
        DuckValue::Decimal(value) => JsonValue::String(value.to_string()),
        DuckValue::Timestamp(unit, value) => {
            let mut object = JsonMap::new();
            object.insert(
                "type".to_string(),
                JsonValue::String(format!("timestamp_{}", time_unit_name(*unit))),
            );
            object.insert("value".to_string(), JsonValue::from(*value));
            JsonValue::Object(object)
        }
        DuckValue::Text(value) => JsonValue::String(value.clone()),
        DuckValue::Blob(value) => JsonValue::Array(
            value
                .iter()
                .map(|byte| JsonValue::from(u64::from(*byte)))
                .collect(),
        ),
        DuckValue::Date32(value) => JsonValue::from(*value),
        DuckValue::Time64(unit, value) => {
            let mut object = JsonMap::new();
            object.insert(
                "type".to_string(),
                JsonValue::String(format!("time64_{}", time_unit_name(*unit))),
            );
            object.insert("value".to_string(), JsonValue::from(*value));
            JsonValue::Object(object)
        }
        DuckValue::Interval {
            months,
            days,
            nanos,
        } => {
            let mut object = JsonMap::new();
            object.insert("months".to_string(), JsonValue::from(*months));
            object.insert("days".to_string(), JsonValue::from(*days));
            object.insert("nanos".to_string(), JsonValue::from(*nanos));
            JsonValue::Object(object)
        }
        DuckValue::List(values) | DuckValue::Array(values) => {
            JsonValue::Array(values.iter().map(duckdb_value_to_json).collect())
        }
        DuckValue::Enum(value) => JsonValue::String(value.clone()),
        DuckValue::Struct(values) => {
            let mut object = JsonMap::new();
            for (key, value) in values.iter() {
                object.insert(key.clone(), duckdb_value_to_json(value));
            }
            JsonValue::Object(object)
        }
        DuckValue::Map(values) => JsonValue::Array(
            values
                .iter()
                .map(|(key, value)| {
                    let mut entry = JsonMap::new();
                    entry.insert("key".to_string(), duckdb_value_to_json(key));
                    entry.insert("value".to_string(), duckdb_value_to_json(value));
                    JsonValue::Object(entry)
                })
                .collect(),
        ),
        DuckValue::Union(value) => duckdb_value_to_json(value.as_ref()),
    }
}

fn json_float(value: f64) -> JsonValue {
    JsonNumber::from_f64(value)
        .map(JsonValue::Number)
        .unwrap_or_else(|| JsonValue::String(value.to_string()))
}

fn time_unit_name(unit: DuckTimeUnit) -> &'static str {
    match unit {
        DuckTimeUnit::Second => "second",
        DuckTimeUnit::Millisecond => "millisecond",
        DuckTimeUnit::Microsecond => "microsecond",
        DuckTimeUnit::Nanosecond => "nanosecond",
    }
}

fn status_internal<E: std::fmt::Display>(prefix: &str, error: E) -> Status {
    Status::internal(format!("{prefix}: {error}"))
}

struct GrpcChunkWriter {
    tx: mpsc::Sender<Result<QueryResponse, Status>>,
    pending: Vec<u8>,
    target_chunk_size: usize,
}

impl GrpcChunkWriter {
    fn new(tx: mpsc::Sender<Result<QueryResponse, Status>>, target_chunk_size: usize) -> Self {
        let chunk_size = target_chunk_size.max(64 * 1024);
        Self {
            tx,
            pending: Vec::with_capacity(chunk_size),
            target_chunk_size: chunk_size,
        }
    }

    fn emit_full_chunks(&mut self) -> io::Result<()> {
        while self.pending.len() >= self.target_chunk_size {
            let remainder = self.pending.split_off(self.target_chunk_size);
            let chunk = std::mem::replace(&mut self.pending, remainder);
            self.send_chunk(chunk)?;
        }
        Ok(())
    }

    fn emit_remaining(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }

        let chunk = std::mem::take(&mut self.pending);
        self.send_chunk(chunk)
    }

    fn send_chunk(&self, chunk: Vec<u8>) -> io::Result<()> {
        self.tx
            .blocking_send(Ok(QueryResponse {
                arrow_ipc_chunk: Bytes::from(chunk),
            }))
            .map_err(|err| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    format!("gRPC stream closed: {err}"),
                )
            })
    }
}

impl Write for GrpcChunkWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        self.pending.extend_from_slice(buf);
        self.emit_full_chunks()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.emit_remaining()
    }
}
