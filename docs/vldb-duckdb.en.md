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
  "threads": 4
}
```

Fields:

- `host`: gRPC bind host
- `port`: gRPC bind port
- `db_path`: DuckDB database file path
- `memory_limit`: DuckDB `PRAGMA memory_limit`
- `threads`: DuckDB `PRAGMA threads`

Config discovery order:

1. `--config <path>` or `-config <path>`
2. `vldb-duckdb.json` in the current working directory
3. `vldb-duckdb.json` in the executable directory
4. built-in defaults

Path handling:

- relative `db_path` values are resolved relative to the config file directory
- absolute paths are supported
- `~` is supported

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
- the service clones a dedicated DuckDB connection per request
- `memory_limit` and `threads` are applied on startup and on cloned request connections
- there is no built-in auth, ACL, or TLS layer
- clients should consume the full stream to receive the complete result
