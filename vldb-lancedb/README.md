# vldb-lancedb

A standalone gRPC service built on LanceDB for vector table management, vector upsert, similarity search, conditional delete, and table drop.

## Config discovery

The server resolves configuration in this order:

1. `-config <path>` or `--config <path>`
2. `<executable_dir>/vldb-lancedb.json`
3. `<executable_dir>/lancedb.json`
4. `./vldb-lancedb.json`
5. `./lancedb.json`
6. built-in defaults

Relative local paths inside the config file are resolved against the config file directory.
URI-like `db_path` values containing `://` are used as-is.

## Default config

```json
{
  "host": "127.0.0.1",
  "port": 50051,
  "db_path": "./data/lancedb"
}
```

## Run

```bash
cargo run --release
```

Or with explicit config:

```bash
cargo run --release -- -config ./vldb-lancedb.json
```

## Docker

Build:

```bash
docker build -t vulcan/vldb-lancedb:local .
```

Run:

```bash
docker run -d \
  --name vldb-lancedb \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  -v ./docker/vldb-lancedb.json:/app/config/vldb-lancedb.json:ro \
  vulcan/vldb-lancedb:local
```

The image uses `docker/vldb-lancedb.json`, which binds to `0.0.0.0` for container networking.
Its Docker-specific `db_path` is `/app/data/lancedb`, so LanceDB writes directly to the mounted volume.
On Docker Desktop for Windows, a Docker named volume is the recommended data mount for LanceDB.

## API summary

- `CreateTable`: create a LanceDB table with scalar and vector columns
- `VectorUpsert`: append or merge JSON / Arrow rows into a table
- `VectorSearch`: run nearest-neighbor search and return JSON or Arrow IPC
- `Delete`: remove rows that match a predicate string
- `DropTable`: remove an entire table

## Notes

- Vector columns currently use fixed-size `float32` lists.
- `Delete.condition` is passed directly to LanceDB as a predicate string.
- Builds require `protoc` during Rust dependency compilation.
- The Go example client under `examples/go-client/` covers create, upsert, search, delete, and drop-table flows.
