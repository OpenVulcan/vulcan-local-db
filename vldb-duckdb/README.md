# vldb-duckdb

A standalone gRPC microservice that wraps DuckDB and exposes parameterized SQL execution, lightweight JSON queries, and Arrow IPC stream queries.

## Config discovery

The server resolves configuration in this order:

1. `-config <path>` or `--config <path>`
2. `./vldb-duckdb.json`
3. `<executable_dir>/vldb-duckdb.json`
4. built-in defaults

Relative paths inside the config file are resolved against the config file directory.
Absolute paths and `~` are supported.

## Default config

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

## Run

```bash
cargo run --release
```

Or with explicit config:

```bash
cargo run --release -- -config ./vldb-duckdb.json
```

## Docker

Build:

```bash
docker build -t vulcan/vldb-duckdb:local .
```

Run:

```bash
docker run -d \
  --name vldb-duckdb \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  -v ./docker/vldb-duckdb.json:/app/config/vldb-duckdb.json:ro \
  vulcan/vldb-duckdb:local
```

The image uses `docker/vldb-duckdb.json`, whose Docker-specific `db_path` is `/app/data/duckdb.db` so DuckDB writes directly to the mounted volume.

## API summary

- `ExecuteScript`: execute DDL / DML / implicit-transaction SQL script without row output, or run a single parameterized SQL statement with `params_json`
- `QueryJson`: execute a lightweight query and return JSON text directly
- `QueryStream`: execute a query and stream Arrow IPC bytes over gRPC

`params_json` must be a JSON array of scalar values such as `[1, "alpha", true]`. When `params_json` is set, `ExecuteScript` only accepts a single SQL statement.

## Notes

- The service keeps one shared DuckDB connection open for the configured database path and serializes request execution through that single connection.
- By default the service acquires an OS-backed lock file next to the database file, for example `duckdb.db.vldb.lock`, so another `vldb-duckdb` process cannot silently reuse the same database path.
- All blocking DuckDB work runs inside `tokio::task::spawn_blocking`.
- When `logging.log_dir` is empty, the server creates a sibling directory with a `_log` suffix, for example `./data/duckdb.db` -> `./data/duckdb_log/`.
- The configured `logging.log_file_name` is treated as the base name, and the service writes daily log files such as `vldb-duckdb_2026-03-31.log`.
- If the client sends `grpc-timeout`, the server now logs that deadline and interrupts the running DuckDB query when the deadline expires.
- Each request now logs request type, remote address, timeout, SQL preview, stage, elapsed time, and final status to help diagnose intermittent timeouts and shared-connection queueing.
- Slow SQL logging is enabled by default for requests that take 1000ms or longer.
- Arrow IPC bytes are chunked in-process to avoid building the full stream in memory before sending.
- Small result sets such as `count(*)` can use `QueryJson` instead of Arrow IPC.
- `memory_limit` and `threads` are applied when the shared DuckDB connection starts.
- The default hardening profile disables external file access and automatic extension installation/loading. If you need `read_csv`, `COPY`, `ATTACH`, or extension workflows, opt in with `hardening.enable_external_access` and optionally whitelist paths through `hardening.allowed_directories` / `hardening.allowed_paths`.
- When DuckDB reports a fatal deadlock-style transaction error, the service now resets the shared connection and returns an error that tells clients whether the commit outcome may already be durable. Treat `ABORTED` writes as "check state before retrying".

## Schema And Storage Pitfalls

- Do not allocate numeric IDs with `SELECT MAX(id) + 1`; use `CREATE SEQUENCE ...` plus `DEFAULT nextval('...')`, or a UUID key, so retries and concurrent writers do not collide.
- DuckDB automatically creates ART indexes for `PRIMARY KEY`, `UNIQUE`, and `FOREIGN KEY` constraints. These are useful, but they also add write cost and can surface eager update conflicts when indexed values are modified in place.
- Prefer append-only immutable surrogate keys. Avoid updating primary keys or unique business keys in place unless you have tested the exact migration path.
- `VACUUM` reclaims row-group space inside the file but does not shrink the file on disk. Use `CHECKPOINT`, or rewrite/copy the database file, when you need physical file compaction.
- Keep migrations simple: adding constraints after the fact, or altering heavily indexed tables, is more fragile than a copy-and-swap migration into a freshly created table.
