# vldb-duckdb Usage Guide

## What It Is

`vldb-duckdb` is a Rust gRPC microservice that wraps DuckDB and exposes three main RPCs:

- `ExecuteScript`: run DDL, DML, or a single parameterized SQL statement without returning rows
- `QueryJson`: execute a lightweight query and return JSON directly
- `QueryStream`: execute a query and stream the result as Arrow IPC chunks

It is useful when you want to:

- expose DuckDB as a local service
- perform parameterized SQL writes instead of unsafe string concatenation
- fetch counters and other small result sets through JSON
- consume query results in Arrow-compatible clients
- call SQL execution from other languages through gRPC

## Key Files

- Project directory: `vldb-duckdb/`
- Example config: `vldb-duckdb/vldb-duckdb.json.example`
- Service entrypoint: `vldb-duckdb/src/main.rs`
- Config loader: `vldb-duckdb/src/config.rs`
- gRPC contract: `vldb-duckdb/proto/v1/duckdb.proto`
- Go example client: `vldb-duckdb/demo/go-client/`

## Requirements

- Rust `1.94.0`
- Go `1.24+` if you want to run the Go example client

## Build

From the project root:

```bash
cd ./vldb-duckdb
cargo build
cargo build --release
```

## Start The Service

1. Prepare a config file:

```bash
cd ./vldb-duckdb
copy .\\vldb-duckdb.json.example .\\vldb-duckdb.json
```

2. Start the service:

```bash
cargo run --release -- --config .\\vldb-duckdb.json
```

If `--config` is omitted, the service falls back to its config discovery order.

## Docker Build And Run

Build the image:

```bash
docker build -t vulcan/vldb-duckdb:local ./vldb-duckdb
```

Run the container:

```bash
docker run -d \
  --name vldb-duckdb \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  -v ./vldb-duckdb/docker/vldb-duckdb.json:/app/config/vldb-duckdb.json:ro \
  vulcan/vldb-duckdb:local
```

Notes:

- the image uses `vldb-duckdb/docker/vldb-duckdb.json`
- the Docker config uses `db_path: "/app/data/duckdb.db"` so DuckDB writes directly to the mounted volume
- DuckDB data is stored under `/app/data` inside the container

## Configuration

Default example:

```json
{
  "host": "0.0.0.0",
  "port": 50052,
  "db_path": "./data/duckdb.db",
  "memory_limit": "2GB",
  "threads": 4,
  "hardening": {
    "enforce_db_file_lock": true,
    "enable_external_access": false,
    "allowed_directories": [],
    "allowed_paths": [],
    "allow_community_extensions": false,
    "autoload_known_extensions": false,
    "autoinstall_known_extensions": false,
    "lock_configuration": true,
    "checkpoint_on_shutdown": true
  },
  "logging": {
    "enabled": true,
    "file_enabled": true,
    "stderr_enabled": true,
    "request_log_enabled": true,
    "slow_query_log_enabled": true,
    "slow_query_threshold_ms": 1000,
    "slow_query_full_sql_enabled": true,
    "sql_preview_chars": 160,
    "log_dir": "",
    "log_file_name": "vldb-duckdb.log"
  }
}
```

Fields:

- `host`: gRPC bind host
- `port`: gRPC bind port
- `db_path`: DuckDB database file path
- `memory_limit`: DuckDB `PRAGMA memory_limit`
- `threads`: DuckDB `PRAGMA threads`
- `hardening.enforce_db_file_lock`: keep a process-level lock file next to the database so another `vldb-duckdb` process cannot reuse the same database path by accident
- `hardening.enable_external_access`: allow SQL to access files other than the primary database file; disabled by default
- `hardening.allowed_directories`: directory allowlist that remains accessible when external access is disabled
- `hardening.allowed_paths`: file allowlist that remains accessible when external access is disabled
- `hardening.allow_community_extensions`: allow community extensions
- `hardening.autoload_known_extensions`: allow automatic loading of known extensions
- `hardening.autoinstall_known_extensions`: allow automatic installation of known extensions
- `hardening.lock_configuration`: lock DuckDB configuration after startup so these guardrails cannot be changed inside the session
- `hardening.checkpoint_on_shutdown`: run a checkpoint on graceful shutdown to reduce leftover WAL state
- `logging.enabled`: master switch for service request logging
- `logging.file_enabled`: write logs to a file under the resolved log directory
- `logging.stderr_enabled`: mirror logs to stderr
- `logging.request_log_enabled`: emit per-request start / success / failure logs
- `logging.slow_query_log_enabled`: emit slow-query logs when a request exceeds the threshold
- `logging.slow_query_threshold_ms`: slow-query threshold in milliseconds, default `1000`
- `logging.slow_query_full_sql_enabled`: log the full SQL text instead of only the preview for slow queries
- `logging.sql_preview_chars`: maximum SQL preview length for normal request logs
- `logging.log_dir`: optional custom log directory; when empty, the service uses a sibling directory with a `_log` suffix based on the DuckDB file stem
- `logging.log_file_name`: base log file name; the service appends the local date before the extension

Config discovery order:

1. `--config <path>` or `-config <path>`
2. `vldb-duckdb.json` in the current working directory
3. `vldb-duckdb.json` in the executable directory
4. built-in defaults

Path handling:

- relative `db_path` values are resolved relative to the config file directory
- absolute paths are supported
- `~` is supported
- when `logging.log_dir` is empty, `./data/duckdb.db` resolves logs to `./data/duckdb_log/`
- daily log files are written as `vldb-duckdb_YYYY-MM-DD.log`
- relative entries in `hardening.allowed_directories` and `hardening.allowed_paths` are also resolved from the config file directory

## How To Call The RPCs

Service name:

- `vldb.duckdb.v1.DuckDbService`

### 1. ExecuteScript

Use it to:

- create or drop tables
- insert or update data
- run multi-statement scripts

Request:

- `sql: string`
- `params_json: string`

Example:

```sql
drop table if exists demo_items;
create table demo_items(id integer, name varchar, active boolean);
insert into demo_items values
  (1, 'alpha', true),
  (2, 'beta', true),
  (3, 'gamma', false);
```

Response:

- `success`
- `message`

Parameter notes:

- when `params_json` is empty, multi-statement scripts are allowed
- when `params_json` is provided, only a single SQL statement is supported
- `params_json` must be a JSON array such as `[1, "alpha", true]`

### 2. QueryJson

Use it to:

- execute lightweight queries
- return JSON text directly for counters and small result sets

Request:

- `sql: string`
- `params_json: string`

Response:

- `json_data: string`

Notes:

- the payload is a JSON array string
- for example, `SELECT count(*) AS total FROM demo_items` returns something like `[{"total":3}]`

### 3. QueryStream

Use it to:

- run SQL queries
- stream result data as Arrow IPC

Request:

- `sql: string`
- `params_json: string`

Response stream:

- `arrow_ipc_chunk: bytes`

Clients must concatenate all chunks and decode the full payload as an Arrow IPC stream.

## Go Example Client

Generate Go stubs:

```bash
cd ./vldb-duckdb/demo/go-client
go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.11
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.6.1
./generate.sh
```

Run the demo:

```bash
go run . -addr 127.0.0.1:50052 -out ./query.arrow.stream
```

The example client:

1. calls `ExecuteScript` to create the demo table
2. inserts rows with `params_json`
3. calls `QueryJson` for `count(*)`
4. calls `QueryStream` for Arrow IPC rows
5. writes `query.arrow.stream`
6. re-opens the file and prints Arrow batch information

## Notes

- `ExecuteScript` is for scripts without row output
- `params_json` currently supports scalar JSON values only: `null`, booleans, numbers, and strings
- `QueryJson` is the lighter option for counters and other small result sets
- `QueryStream` returns Arrow IPC bytes, not JSON
- the service keeps one shared DuckDB connection open and serializes request execution through that single connection
- by default the service also holds a sibling lock file such as `duckdb.db.vldb.lock` so a second `vldb-duckdb` process cannot silently target the same database file
- `memory_limit` and `threads` are applied when the shared DuckDB connection starts
- the default hardening profile disables external file access, community extensions, and automatic extension installation/loading, then locks the relevant DuckDB settings; only opt out if you explicitly need `read_csv`, `COPY`, `ATTACH`, or extension workflows
- timeout and slow-query logs now include the last execution stage, such as `waiting_for_connection`, `acquiring_connection_lock`, `preparing_statement`, `executing_query`, `fetching_rows`, or `serializing_json`
- request logging and slow-query logging are enabled by default
- there is no built-in auth, ACL, or TLS layer
- clients should consume the full stream to receive the complete result
- if a write returns `ABORTED`, treat it as "commit outcome may already be durable" and inspect state before retrying

## Schema Risk Checklist

- Do not allocate numeric IDs with `SELECT MAX(id) + 1`. Prefer `CREATE SEQUENCE ...` plus `DEFAULT nextval('...')`, or use UUID primary keys.
- DuckDB automatically creates ART indexes for `PRIMARY KEY`, `UNIQUE`, and `FOREIGN KEY`. They enforce correctness, but they also increase write cost and can surface conflicts when constrained values are updated in place.
- Prefer immutable surrogate keys. If a business identifier must change, use a new column or a copy-and-swap migration instead of mass-updating primary keys in place.
- `VACUUM` does not physically shrink the database file at the filesystem level. After large deletes or rewrites, pair maintenance with `CHECKPOINT`, or export/rebuild the file when you need real disk compaction.
- For heavy schema changes on indexed tables, prefer "create new table -> backfill -> switch over" migrations instead of complex in-place `ALTER TABLE` sequences.
- Keep time columns in a consistent UTC representation unless you explicitly need session time zone semantics. `TIMESTAMPTZ` is powerful, but mixed client time zones can change date boundaries in surprising ways.
