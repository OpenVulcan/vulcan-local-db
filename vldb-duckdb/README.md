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
  "threads": 4
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

- The root `duckdb::Connection` is stored behind `Arc<Mutex<Connection>>`, but each request clones a dedicated DuckDB connection with `try_clone()` so the mutex is only held briefly.
- All blocking DuckDB work runs inside `tokio::task::spawn_blocking`.
- Arrow IPC bytes are chunked in-process to avoid building the full stream in memory before sending.
- Small result sets such as `count(*)` can use `QueryJson` instead of Arrow IPC.
- `memory_limit` and `threads` are applied on startup and again on per-request cloned connections.
