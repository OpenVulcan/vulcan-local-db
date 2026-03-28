# VulcanLocalDb

English | [简体中文](./README.zh-CN.md)

VulcanLocalDb is a local-first data gateway workspace for applications and AI agents that need fast, structured access to private data without pushing everything into a remote service. It packages two Rust gRPC services:

- `vldb-lancedb`: a vector data gateway built on LanceDB
- `vldb-duckdb`: a SQL and analytics gateway built on DuckDB

Together they give you a simple local deployment model:

- store and search embeddings through LanceDB
- run parameterized SQL safely through DuckDB
- use lightweight JSON for small results and Arrow IPC for large result sets
- integrate from other languages through stable gRPC APIs

## What This Repository Contains

| Service | Purpose | Typical Use |
| --- | --- | --- |
| `vldb-lancedb` | Vector table management, vector upsert, nearest-neighbor search, conditional delete, and table drop | Agent memory, local RAG, semantic search, forgetting and cleanup |
| `vldb-duckdb` | Parameterized SQL execution, lightweight JSON queries, and Arrow IPC streaming | Local analytics, counters, tabular query APIs, ETL helpers |

Both services include Go demo clients and Docker packaging.

## Why This Project Exists

This repository is designed for scenarios where you want a small local gateway instead of a large application server:

- desktop or edge deployments that need embedded data services
- AI assistants that need local vector memory and SQL access
- internal tools that prefer gRPC over direct database coupling
- services that want one path for lightweight JSON results and another for high-volume Arrow data

## Quick Start

### Start With Docker

```bash
docker compose -f ./docker-compose.example.yml up -d --build
```

Default ports:

- `vldb-lancedb`: `127.0.0.1:50051`
- `vldb-duckdb`: `127.0.0.1:50052`

Stop the stack:

```bash
docker compose -f ./docker-compose.example.yml down
```

### Build Locally

```bash
cd ./vldb-lancedb
cargo build

cd ../vldb-duckdb
cargo build
```

## Repository Layout

```text
.
|-- vldb-lancedb/
|-- vldb-duckdb/
|-- docs/
|-- docker-compose.example.yml
|-- README.md
`-- README.zh-CN.md
```

## Documentation

- Chinese overview: [README.zh-CN.md](./README.zh-CN.md)
- Documentation index: [docs/README.en.md](./docs/README.en.md)
- Docker guide:
  - English: [docs/docker.en.md](./docs/docker.en.md)
  - Chinese: [docs/docker.zh-CN.md](./docs/docker.zh-CN.md)
- Service guides:
  - `vldb-lancedb`
    - English: [docs/vldb-lancedb.en.md](./docs/vldb-lancedb.en.md)
    - Chinese: [docs/vldb-lancedb.zh-CN.md](./docs/vldb-lancedb.zh-CN.md)
  - `vldb-duckdb`
    - English: [docs/vldb-duckdb.en.md](./docs/vldb-duckdb.en.md)
    - Chinese: [docs/vldb-duckdb.zh-CN.md](./docs/vldb-duckdb.zh-CN.md)

## Current Status

- both Rust services build in `debug` and `release`
- both Go demo clients build and run
- both services have been smoke-tested end-to-end through local gRPC clients
- Docker packaging is available for both services

## Notes

- this repository exposes gRPC services, not REST APIs
- `vldb-lancedb` requires `protoc` during Rust builds
- `vldb-duckdb` supports both `QueryJson` and `QueryStream`
- on Docker Desktop for Windows, `vldb-lancedb` is best persisted with Docker named volumes

## License

This repository is distributed under the [LICENSE](./LICENSE).
