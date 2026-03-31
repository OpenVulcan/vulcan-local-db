# vldb-lancedb Usage Guide

## What It Is

`vldb-lancedb` is a Rust gRPC service built around LanceDB. It provides five main capabilities:

- table creation with scalar and vector columns
- vector data ingestion from JSON rows or Arrow IPC
- vector similarity search with JSON or Arrow IPC output
- conditional row deletion
- full table removal

It is useful when you want to:

- run LanceDB as a local service
- call vector ingestion and search from other languages via gRPC
- manage vector tables with both embeddings and metadata fields

## Key Files

- Project directory: `vldb-lancedb/`
- Example config: `vldb-lancedb/vldb-lancedb.json.example`
- Service entrypoint: `vldb-lancedb/src/main.rs`
- Config loader: `vldb-lancedb/src/config.rs`
- gRPC contract: `vldb-lancedb/proto/v1/lancedb.proto`
- Go example client: `vldb-lancedb/examples/go-client/`

## Requirements

- Rust `1.94.0`
- Go `1.24+` if you want to run the Go example
- `protoc`

Notes:

- the `lance-*` dependency chain uses `protoc` during builds
- if `protoc` is not available on `PATH`, set the `PROTOC` environment variable explicitly

## Build

```bash
cd ./vldb-lancedb
cargo build
cargo build --release
```

## Start The Service

1. Prepare a config file:

```bash
cd ./vldb-lancedb
copy .\\vldb-lancedb.json.example .\\vldb-lancedb.json
```

2. Start the service:

```bash
cargo run --release -- --config .\\vldb-lancedb.json
```

## Docker Build And Run

Build the image:

```bash
docker build -t vulcan/vldb-lancedb:local ./vldb-lancedb
```

Run the container:

```bash
docker run -d \
  --name vldb-lancedb \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  -v ./vldb-lancedb/docker/vldb-lancedb.json:/app/config/vldb-lancedb.json:ro \
  vulcan/vldb-lancedb:local
```

Notes:

- the image uses `vldb-lancedb/docker/vldb-lancedb.json`
- the Docker config binds to `0.0.0.0`
- the Docker config uses `db_path: "/app/data"` so LanceDB writes directly to the mounted volume root
- LanceDB data is stored under `/app/data` inside the container
- on Docker Desktop for Windows, a Docker named volume is the recommended persistence option

## Configuration

Default example:

```json
{
  "host": "127.0.0.1",
  "port": 50051,
  "db_path": "./data",
  "logging": {
    "enabled": true,
    "file_enabled": true,
    "stderr_enabled": true,
    "request_log_enabled": true,
    "slow_request_log_enabled": true,
    "slow_request_threshold_ms": 1000,
    "include_request_details_in_slow_log": true,
    "request_preview_chars": 160,
    "log_dir": "",
    "log_file_name": "vldb-lancedb.log"
  }
}
```

Fields:

- `host`: gRPC bind host
- `port`: gRPC bind port
- `db_path`: LanceDB directory path or remote URI
- `logging.enabled`: master switch for service request logging
- `logging.file_enabled`: write logs to a file under the resolved log directory
- `logging.stderr_enabled`: mirror logs to stderr
- `logging.request_log_enabled`: emit per-request start / success / failure logs
- `logging.slow_request_log_enabled`: emit slow-request logs when a request exceeds the threshold
- `logging.slow_request_threshold_ms`: slow-request threshold in milliseconds, default `1000`
- `logging.include_request_details_in_slow_log`: include request summary in slow-request logs
- `logging.request_preview_chars`: maximum preview length for filter / request summaries
- `logging.log_dir`: optional custom log directory; when empty and `db_path` is local, the service uses `<db_path>/logs/`
- `logging.log_file_name`: base log file name; the service appends the local date before the extension

Config discovery order:

1. `--config <path>` or `-config <path>`
2. `vldb-lancedb.json` in the executable directory
3. `lancedb.json` in the executable directory
4. `vldb-lancedb.json` in the current working directory
5. `lancedb.json` in the current working directory
6. built-in defaults

Path handling:

- relative local `db_path` values are resolved relative to the config file directory
- URI-like values containing `://` are used as-is
- local directories are created automatically if they do not exist
- when `logging.log_dir` is empty and `db_path` is local, logs are stored under `<db_path>/logs/`
- daily log files are written as `vldb-lancedb_YYYY-MM-DD.log`

## How To Call The RPCs

Service name:

- `vldb.lancedb.v1.LanceDbService`

### 1. CreateTable

Use it to:

- create a new table
- define scalar columns and vector columns

Key fields:

- `table_name`
- `columns`
- `overwrite_if_exists`

Vector column rules:

- use `COLUMN_TYPE_VECTOR_FLOAT32`
- `vector_dim` must be greater than `0`

### 2. VectorUpsert

Use it to:

- append or merge data into an existing table
- write either JSON rows or Arrow IPC batches

Key fields:

- `table_name`
- `input_format`
- `data`
- `key_columns`

Input formats:

- `INPUT_FORMAT_JSON_ROWS`
- `INPUT_FORMAT_ARROW_IPC`

Behavior:

- empty `key_columns` means append mode
- non-empty `key_columns` means merge upsert mode

JSON ingestion rules:

- `data` must be a JSON array
- every row must be a JSON object
- vector fields must be arrays of float values
- non-nullable fields must be present

### 3. VectorSearch

Use it to:

- run nearest-neighbor vector search

Key fields:

- `table_name`
- `vector`
- `limit`
- `filter`
- `vector_column`
- `output_format`

Output formats:

- `OUTPUT_FORMAT_JSON_ROWS`
- `OUTPUT_FORMAT_ARROW_IPC`

Notes:

- `vector` must not be empty
- when `limit=0`, the service falls back to `10`
- `filter` can be used for additional conditions such as `active = true`

### 4. Delete

Use it to:

- delete rows that match a predicate
- implement forgetting and cleanup flows

Key fields:

- `table_name`
- `condition`

Example predicates:

- `session_id = 'abc'`
- `id >= 100`

Response fields:

- `success`
- `message`
- `version`
- `deleted_rows`

### 5. DropTable

Use it to:

- remove an entire LanceDB table

Key fields:

- `table_name`

Response fields:

- `success`
- `message`

## Go Example Client

Generate Go stubs:

```bash
go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.11
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.6.1

protoc \
  -I . \
  --go_out=./examples/go-client/gen \
  --go_opt=paths=source_relative \
  --go-grpc_out=./examples/go-client/gen \
  --go-grpc_opt=paths=source_relative \
  ./proto/v1/lancedb.proto
```

Run the Go example:

```bash
cd ./examples/go-client
go mod tidy
go run .
```

The example runs:

1. `CreateTable`
2. `VectorUpsert`
3. `VectorSearch`
4. `Delete`
5. `DropTable`

## Notes

- there is no built-in auth, ACL, or TLS layer
- vector columns are currently handled as fixed-size `float32` lists
- JSON field types must match the table schema
- JSON search output may include `_distance`
- `Delete.condition` is passed through to LanceDB as a predicate string, so callers must provide a valid filter expression
- request logging and slow-request logging are enabled by default
- Rust builds will fail if `protoc` is missing
