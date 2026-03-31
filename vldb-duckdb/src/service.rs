use crate::config::Config;
use crate::logging::ServiceLogger;
use crate::pb::duck_db_service_server::DuckDbService;
use crate::pb::{ExecuteRequest, ExecuteResponse, QueryJsonResponse, QueryRequest, QueryResponse};
use arrow::ipc::writer::StreamWriter;
use bytes::Bytes;
use duckdb::types::{
    TimeUnit as DuckTimeUnit, ToSql, Value as DuckValue, ValueRef as DuckValueRef,
};
use duckdb::{Connection, InterruptHandle};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::io;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

const STREAM_CHANNEL_CAPACITY: usize = 8;
const DEFAULT_IPC_CHUNK_BYTES: usize = 1024 * 1024;
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct AppState {
    root_connection: Arc<Mutex<Connection>>,
    execution_gate: Arc<Semaphore>,
    logger: Arc<ServiceLogger>,
}

#[derive(Clone, Debug)]
pub struct DuckDbGrpcService {
    state: Arc<AppState>,
}

#[derive(Clone, Debug)]
struct RequestLogContext {
    logger: Arc<ServiceLogger>,
    progress: Arc<RequestProgress>,
    request_id: u64,
    operation: &'static str,
    remote_addr: Option<SocketAddr>,
    grpc_timeout: Option<Duration>,
    started_at: Instant,
    sql_full: String,
    sql_preview: String,
    params_json_bytes: usize,
    request_log_enabled: bool,
    slow_query_log_enabled: bool,
    slow_query_threshold: Duration,
    slow_query_full_sql_enabled: bool,
}

#[derive(Debug)]
struct RequestProgress {
    stage: Mutex<&'static str>,
}

impl RequestProgress {
    fn new(initial_stage: &'static str) -> Self {
        Self {
            stage: Mutex::new(initial_stage),
        }
    }

    fn set(&self, stage: &'static str) {
        if let Ok(mut guard) = self.stage.lock() {
            *guard = stage;
        }
    }

    fn snapshot(&self) -> &'static str {
        self.stage.lock().map(|guard| *guard).unwrap_or("unknown")
    }
}

struct WorkerCompletionSignal(Option<oneshot::Sender<()>>);

impl WorkerCompletionSignal {
    fn new(tx: oneshot::Sender<()>) -> Self {
        Self(Some(tx))
    }
}

impl Drop for WorkerCompletionSignal {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

impl DuckDbGrpcService {
    pub fn new(root_connection: Connection, logger: Arc<ServiceLogger>) -> Self {
        Self {
            state: Arc::new(AppState {
                root_connection: Arc::new(Mutex::new(root_connection)),
                execution_gate: Arc::new(Semaphore::new(1)),
                logger,
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
        let context = build_request_context(
            &self.state,
            &request,
            "execute_script",
            request.get_ref().sql.as_str(),
            request.get_ref().params_json.as_str(),
        );
        log_request_started(&context);

        let req = request.into_inner();
        let sql = req.sql;
        if sql.trim().is_empty() {
            log_request_invalid_argument(&context, "sql must not be empty");
            return Err(Status::invalid_argument("sql must not be empty"));
        }

        let permit = acquire_execution_permit(&context, &self.state)
            .await
            .inspect_err(|status| log_request_failed(&context, status))?;
        let state = Arc::clone(&self.state);
        let deadline_triggered = Arc::new(AtomicBool::new(false));
        let (interrupt_tx, interrupt_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();

        spawn_deadline_interrupt_watcher(
            context.clone(),
            interrupt_rx,
            done_rx,
            Arc::clone(&deadline_triggered),
        );

        let worker_context = context.clone();
        let response = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _completion = WorkerCompletionSignal::new(done_tx);
            run_execute_script(
                worker_context,
                state,
                sql,
                req.params_json,
                Some(interrupt_tx),
            )
        })
        .await
        .map_err(|err| Status::internal(format!("execute worker join failed: {err}")))?;
        let response = remap_deadline_status_if_needed(response, &deadline_triggered)?;

        Ok(Response::new(response))
    }

    type QueryStreamStream = ReceiverStream<Result<QueryResponse, Status>>;

    async fn query_stream(
        &self,
        request: Request<QueryRequest>,
    ) -> Result<Response<Self::QueryStreamStream>, Status> {
        let context = build_request_context(
            &self.state,
            &request,
            "query_stream",
            request.get_ref().sql.as_str(),
            request.get_ref().params_json.as_str(),
        );
        log_request_started(&context);

        let req = request.into_inner();
        let sql = req.sql;
        if sql.trim().is_empty() {
            log_request_invalid_argument(&context, "sql must not be empty");
            return Err(Status::invalid_argument("sql must not be empty"));
        }

        let permit = acquire_execution_permit(&context, &self.state)
            .await
            .inspect_err(|status| log_request_failed(&context, status))?;
        let (tx, rx) = mpsc::channel(STREAM_CHANNEL_CAPACITY);
        let worker_tx = tx.clone();
        let join_tx = tx.clone();
        let state = Arc::clone(&self.state);
        let deadline_triggered = Arc::new(AtomicBool::new(false));
        let (interrupt_tx, interrupt_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();

        spawn_deadline_interrupt_watcher(
            context.clone(),
            interrupt_rx,
            done_rx,
            Arc::clone(&deadline_triggered),
        );

        let worker_context = context.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _completion = WorkerCompletionSignal::new(done_tx);
            run_query_streaming(
                worker_context,
                state,
                sql,
                req.params_json,
                worker_tx,
                Some(interrupt_tx),
            )
        });

        let join_context = context.clone();
        let join_deadline_triggered = Arc::clone(&deadline_triggered);
        tokio::spawn(async move {
            match worker.await {
                Ok(Ok(())) => {}
                Ok(Err(status)) => {
                    let mapped = remap_deadline_status(status, &join_deadline_triggered);
                    let _ = join_tx.send(Err(mapped)).await;
                }
                Err(err) => {
                    let status = Status::internal(format!("query worker join failed: {err}"));
                    log_request_failed(&join_context, &status);
                    let _ = join_tx.send(Err(status)).await;
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
        let context = build_request_context(
            &self.state,
            &request,
            "query_json",
            request.get_ref().sql.as_str(),
            request.get_ref().params_json.as_str(),
        );
        log_request_started(&context);

        let req = request.into_inner();
        if req.sql.trim().is_empty() {
            log_request_invalid_argument(&context, "sql must not be empty");
            return Err(Status::invalid_argument("sql must not be empty"));
        }

        let permit = acquire_execution_permit(&context, &self.state)
            .await
            .inspect_err(|status| log_request_failed(&context, status))?;
        let state = Arc::clone(&self.state);
        let deadline_triggered = Arc::new(AtomicBool::new(false));
        let (interrupt_tx, interrupt_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();

        spawn_deadline_interrupt_watcher(
            context.clone(),
            interrupt_rx,
            done_rx,
            Arc::clone(&deadline_triggered),
        );

        let worker_context = context.clone();
        let response = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _completion = WorkerCompletionSignal::new(done_tx);
            run_query_json(
                worker_context,
                state,
                req.sql,
                req.params_json,
                Some(interrupt_tx),
            )
        })
        .await
        .map_err(|err| Status::internal(format!("query_json worker join failed: {err}")))?;
        let response = remap_deadline_status_if_needed(response, &deadline_triggered)?;

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

async fn acquire_execution_permit(
    context: &RequestLogContext,
    state: &Arc<AppState>,
) -> Result<OwnedSemaphorePermit, Status> {
    set_request_stage(context, "waiting_for_connection");
    let acquire = Arc::clone(&state.execution_gate).acquire_owned();

    if let Some(grpc_timeout) = context.grpc_timeout {
        let deadline = tokio::time::Instant::from_std(context.started_at + grpc_timeout);
        match tokio::time::timeout_at(deadline, acquire).await {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(_)) => Err(Status::internal("duckdb execution gate is closed")),
            Err(_) => {
                log_request_timeout(context);
                Err(Status::deadline_exceeded(
                    "DuckDB request exceeded the gRPC deadline while waiting for the shared connection",
                ))
            }
        }
    } else {
        acquire
            .await
            .map_err(|_| Status::internal("duckdb execution gate is closed"))
    }
}

fn lock_shared_connection<'a>(
    state: &'a Arc<AppState>,
) -> Result<std::sync::MutexGuard<'a, Connection>, Status> {
    state
        .root_connection
        .lock()
        .map_err(|_| Status::internal("duckdb root connection mutex is poisoned"))
}

fn run_execute_script(
    context: RequestLogContext,
    state: Arc<AppState>,
    sql: String,
    params_json: String,
    interrupt_tx: Option<oneshot::Sender<Arc<InterruptHandle>>>,
) -> Result<ExecuteResponse, Status> {
    let result = (|| {
        set_request_stage(&context, "acquiring_connection_lock");
        let conn = lock_shared_connection(&state)?;
        if let Some(tx) = interrupt_tx {
            let _ = tx.send(conn.interrupt_handle());
        }
        set_request_stage(&context, "parsing_params");
        let bound_values = parse_bound_params(&params_json)?;

        if bound_values.is_empty() {
            set_request_stage(&context, "executing_batch");
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

        set_request_stage(&context, "preparing_statement");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|err| status_internal("duckdb prepare failed", err))?;
        let params = bind_values_as_params(&bound_values);
        set_request_stage(&context, "executing_statement");
        let rows_changed = stmt
            .execute(params.as_slice())
            .map_err(|err| status_internal("duckdb execute failed", err))?;

        Ok(ExecuteResponse {
            success: true,
            message: format!("statement executed successfully (rows_changed={rows_changed})"),
        })
    })();

    match &result {
        Ok(response) => log_request_succeeded(&context, response.message.as_str()),
        Err(status) => log_request_failed(&context, status),
    }

    result
}

fn run_query_streaming(
    context: RequestLogContext,
    state: Arc<AppState>,
    sql: String,
    params_json: String,
    tx: mpsc::Sender<Result<QueryResponse, Status>>,
    interrupt_tx: Option<oneshot::Sender<Arc<InterruptHandle>>>,
) -> Result<(), Status> {
    let result = (|| {
        set_request_stage(&context, "acquiring_connection_lock");
        let conn = lock_shared_connection(&state)?;
        if let Some(tx) = interrupt_tx {
            let _ = tx.send(conn.interrupt_handle());
        }
        set_request_stage(&context, "parsing_params");
        let bound_values = parse_bound_params(&params_json)?;
        set_request_stage(&context, "preparing_statement");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|err| status_internal("duckdb prepare failed", err))?;
        let params = bind_values_as_params(&bound_values);

        set_request_stage(&context, "executing_query");
        let mut batches = stmt
            .query_arrow(params.as_slice())
            .map_err(|err| status_internal("duckdb query_arrow failed", err))?;

        let schema = batches.get_schema();
        let chunk_writer = GrpcChunkWriter::new(tx, DEFAULT_IPC_CHUNK_BYTES);
        let mut ipc_writer = StreamWriter::try_new(chunk_writer, &schema)
            .map_err(|err| status_internal("arrow stream header write failed", err))?;

        set_request_stage(&context, "writing_arrow_stream");
        ipc_writer
            .flush()
            .map_err(|err| status_internal("arrow stream flush failed", err))?;

        let mut batch_count = 0usize;
        let mut row_count = 0usize;
        for batch in &mut batches {
            batch_count += 1;
            row_count += batch.num_rows();
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

        let metrics = ipc_writer.get_ref().metrics();
        log_request_succeeded(
            &context,
            format!(
                "streamed {row_count} rows across {batch_count} batches ({chunks} chunks, {bytes} bytes)",
                chunks = metrics.emitted_chunks,
                bytes = metrics.emitted_bytes,
            ),
        );

        Ok(())
    })();

    if let Err(status) = &result {
        log_request_failed(&context, status);
    }

    result
}

fn run_query_json(
    context: RequestLogContext,
    state: Arc<AppState>,
    sql: String,
    params_json: String,
    interrupt_tx: Option<oneshot::Sender<Arc<InterruptHandle>>>,
) -> Result<QueryJsonResponse, Status> {
    let result = (|| {
        set_request_stage(&context, "acquiring_connection_lock");
        let conn = lock_shared_connection(&state)?;
        if let Some(tx) = interrupt_tx {
            let _ = tx.send(conn.interrupt_handle());
        }
        set_request_stage(&context, "parsing_params");
        let bound_values = parse_bound_params(&params_json)?;
        set_request_stage(&context, "preparing_statement");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|err| status_internal("duckdb prepare failed", err))?;
        let params = bind_values_as_params(&bound_values);
        set_request_stage(&context, "executing_query");
        let mut rows = stmt
            .query(params.as_slice())
            .map_err(|err| status_internal("duckdb query failed", err))?;
        let column_names = rows
            .as_ref()
            .ok_or_else(|| Status::internal("duckdb rows lost statement metadata"))?
            .column_names();

        let mut json_rows = Vec::<JsonValue>::new();
        set_request_stage(&context, "fetching_rows");
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

        set_request_stage(&context, "serializing_json");
        let json_data = serde_json::to_string(&json_rows)
            .map_err(|err| status_internal("serialize JSON result failed", err))?;

        Ok(QueryJsonResponse { json_data })
    })();

    match &result {
        Ok(response) => log_request_succeeded(
            &context,
            format!("returned JSON payload ({} bytes)", response.json_data.len()),
        ),
        Err(status) => log_request_failed(&context, status),
    }

    result
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

fn build_request_context<T>(
    state: &Arc<AppState>,
    request: &Request<T>,
    operation: &'static str,
    sql: &str,
    params_json: &str,
) -> RequestLogContext {
    let logging = state.logger.config();
    RequestLogContext {
        logger: Arc::clone(&state.logger),
        progress: Arc::new(RequestProgress::new("queued")),
        request_id: NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed),
        operation,
        remote_addr: request.remote_addr(),
        grpc_timeout: request
            .metadata()
            .get("grpc-timeout")
            .and_then(|value| value.to_str().ok())
            .and_then(parse_grpc_timeout_header),
        started_at: Instant::now(),
        sql_full: sql.trim().to_string(),
        sql_preview: preview_sql(sql, logging.sql_preview_chars),
        params_json_bytes: params_json.len(),
        request_log_enabled: logging.request_log_enabled,
        slow_query_log_enabled: logging.slow_query_log_enabled,
        slow_query_threshold: Duration::from_millis(logging.slow_query_threshold_ms),
        slow_query_full_sql_enabled: logging.slow_query_full_sql_enabled,
    }
}

fn set_request_stage(context: &RequestLogContext, stage: &'static str) {
    context.progress.set(stage);
}

fn spawn_deadline_interrupt_watcher(
    context: RequestLogContext,
    interrupt_rx: oneshot::Receiver<Arc<InterruptHandle>>,
    done_rx: oneshot::Receiver<()>,
    deadline_triggered: Arc<AtomicBool>,
) {
    let Some(grpc_timeout) = context.grpc_timeout else {
        return;
    };

    let deadline = tokio::time::Instant::from_std(context.started_at + grpc_timeout);
    tokio::spawn(async move {
        let Ok(interrupt_handle) = interrupt_rx.await else {
            return;
        };
        let mut done_rx = done_rx;

        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                deadline_triggered.store(true, Ordering::Relaxed);
                log_request_timeout(&context);
                interrupt_handle.interrupt();
                let _ = done_rx.await;
            }
            _ = &mut done_rx => {}
        }
    });
}

fn remap_deadline_status_if_needed<T>(
    result: Result<T, Status>,
    deadline_triggered: &Arc<AtomicBool>,
) -> Result<T, Status> {
    result.map_err(|status| remap_deadline_status(status, deadline_triggered))
}

fn remap_deadline_status(status: Status, deadline_triggered: &Arc<AtomicBool>) -> Status {
    if deadline_triggered.load(Ordering::Relaxed)
        && status.message().to_ascii_lowercase().contains("interrupt")
    {
        return Status::deadline_exceeded(
            "DuckDB query exceeded the gRPC deadline and was interrupted",
        );
    }

    status
}

fn preview_sql(sql: &str, max_chars: usize) -> String {
    let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");
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

fn parse_grpc_timeout_header(raw: &str) -> Option<Duration> {
    let unit = raw.chars().last()?;
    let digits = raw.get(..raw.len().checked_sub(1)?)?;
    let value = digits.parse::<u64>().ok()?;

    match unit {
        'H' => value.checked_mul(60 * 60).map(Duration::from_secs),
        'M' => value.checked_mul(60).map(Duration::from_secs),
        'S' => Some(Duration::from_secs(value)),
        'm' => Some(Duration::from_millis(value)),
        'u' => Some(Duration::from_micros(value)),
        'n' => Some(Duration::from_nanos(value)),
        _ => None,
    }
}

fn log_request_started(context: &RequestLogContext) {
    if !context.request_log_enabled {
        return;
    }

    context.logger.log(
        "start",
        format!(
            "request_id={} op={} remote={} grpc_timeout={} params_json_bytes={} sql=\"{}\"",
            context.request_id,
            context.operation,
            format_remote_addr(context.remote_addr),
            format_optional_duration(context.grpc_timeout),
            context.params_json_bytes,
            context.sql_preview,
        ),
    );
}

fn log_request_invalid_argument(context: &RequestLogContext, message: &str) {
    if context.request_log_enabled {
        context.logger.log(
            "invalid",
            format!(
                "request_id={} op={} elapsed_ms={} remote={} message={} sql=\"{}\"",
                context.request_id,
                context.operation,
                context.started_at.elapsed().as_millis(),
                format_remote_addr(context.remote_addr),
                message,
                context.sql_preview,
            ),
        );
    }
}

fn log_request_timeout(context: &RequestLogContext) {
    context.logger.log(
        "timeout",
        format!(
            "request_id={} op={} elapsed_ms={} remote={} grpc_timeout={} stage={} sql=\"{}\" message=interrupting running DuckDB query because the gRPC deadline expired",
            context.request_id,
            context.operation,
            context.started_at.elapsed().as_millis(),
            format_remote_addr(context.remote_addr),
            format_optional_duration(context.grpc_timeout),
            context.progress.snapshot(),
            context.sql_preview,
        ),
    );
}

fn log_request_succeeded(context: &RequestLogContext, detail: impl AsRef<str>) {
    let elapsed = context.started_at.elapsed();
    if context.request_log_enabled {
        context.logger.log(
            "ok",
            format!(
                "request_id={} op={} elapsed_ms={} remote={} stage={} detail={} sql=\"{}\"",
                context.request_id,
                context.operation,
                elapsed.as_millis(),
                format_remote_addr(context.remote_addr),
                context.progress.snapshot(),
                detail.as_ref(),
                context.sql_preview,
            ),
        );
    }
    maybe_log_slow_query(context, elapsed, "completed", detail.as_ref());
}

fn log_request_failed(context: &RequestLogContext, status: &Status) {
    let elapsed = context.started_at.elapsed();
    context.logger.log(
        "error",
        format!(
            "request_id={} op={} elapsed_ms={} remote={} stage={} code={:?} message={} sql=\"{}\"",
            context.request_id,
            context.operation,
            elapsed.as_millis(),
            format_remote_addr(context.remote_addr),
            context.progress.snapshot(),
            status.code(),
            status.message(),
            context.sql_preview,
        ),
    );
    maybe_log_slow_query(context, elapsed, "failed", status.message());
}

fn maybe_log_slow_query(
    context: &RequestLogContext,
    elapsed: Duration,
    final_state: &str,
    detail: &str,
) {
    if !context.slow_query_log_enabled || elapsed < context.slow_query_threshold {
        return;
    }

    let sql_text = if context.slow_query_full_sql_enabled {
        context.sql_full.as_str()
    } else {
        context.sql_preview.as_str()
    };

    context.logger.log(
        "slow_query",
        format!(
            "request_id={} op={} elapsed_ms={} threshold_ms={} remote={} stage={} state={} detail={} sql=\"{}\"",
            context.request_id,
            context.operation,
            elapsed.as_millis(),
            context.slow_query_threshold.as_millis(),
            format_remote_addr(context.remote_addr),
            context.progress.snapshot(),
            final_state,
            detail,
            sql_text,
        ),
    );
}

fn format_remote_addr(remote_addr: Option<SocketAddr>) -> String {
    remote_addr
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_optional_duration(duration: Option<Duration>) -> String {
    duration
        .map(|value| format!("{}ms", value.as_millis()))
        .unwrap_or_else(|| "none".to_string())
}

struct GrpcChunkWriter {
    tx: mpsc::Sender<Result<QueryResponse, Status>>,
    pending: Vec<u8>,
    target_chunk_size: usize,
    emitted_chunks: usize,
    emitted_bytes: usize,
}

#[derive(Copy, Clone, Debug)]
struct StreamMetrics {
    emitted_chunks: usize,
    emitted_bytes: usize,
}

impl GrpcChunkWriter {
    fn new(tx: mpsc::Sender<Result<QueryResponse, Status>>, target_chunk_size: usize) -> Self {
        let chunk_size = target_chunk_size.max(64 * 1024);
        Self {
            tx,
            pending: Vec::with_capacity(chunk_size),
            target_chunk_size: chunk_size,
            emitted_chunks: 0,
            emitted_bytes: 0,
        }
    }

    fn metrics(&self) -> StreamMetrics {
        StreamMetrics {
            emitted_chunks: self.emitted_chunks,
            emitted_bytes: self.emitted_bytes,
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

    fn send_chunk(&mut self, chunk: Vec<u8>) -> io::Result<()> {
        let chunk_len = chunk.len();
        self.tx
            .blocking_send(Ok(QueryResponse {
                arrow_ipc_chunk: Bytes::from(chunk),
            }))
            .map_err(|err| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    format!("gRPC stream closed: {err}"),
                )
            })?;
        self.emitted_chunks += 1;
        self.emitted_bytes += chunk_len;

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::{parse_grpc_timeout_header, preview_sql};
    use std::time::Duration;

    #[test]
    fn parse_grpc_timeout_supports_all_units() {
        assert_eq!(
            parse_grpc_timeout_header("2H"),
            Some(Duration::from_secs(7200))
        );
        assert_eq!(
            parse_grpc_timeout_header("3M"),
            Some(Duration::from_secs(180))
        );
        assert_eq!(
            parse_grpc_timeout_header("4S"),
            Some(Duration::from_secs(4))
        );
        assert_eq!(
            parse_grpc_timeout_header("5m"),
            Some(Duration::from_millis(5))
        );
        assert_eq!(
            parse_grpc_timeout_header("6u"),
            Some(Duration::from_micros(6))
        );
        assert_eq!(
            parse_grpc_timeout_header("7n"),
            Some(Duration::from_nanos(7))
        );
    }

    #[test]
    fn parse_grpc_timeout_rejects_invalid_values() {
        assert_eq!(parse_grpc_timeout_header(""), None);
        assert_eq!(parse_grpc_timeout_header("abc"), None);
        assert_eq!(parse_grpc_timeout_header("10x"), None);
    }

    #[test]
    fn preview_sql_compacts_whitespace_and_truncates() {
        let preview = preview_sql("select   *\nfrom   demo\twhere id = 1", 160);
        assert_eq!(preview, "select * from demo where id = 1");

        let long_sql = format!("select {}", "x".repeat(300));
        let preview = preview_sql(&long_sql, 32);
        assert!(preview.ends_with("..."));
        assert!(preview.len() > 10);
    }
}
