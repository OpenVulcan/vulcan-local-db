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

## Quick Install

### Docker Hub

Pull and run the published images directly:

```bash
docker pull openvulcan/vldb-lancedb:latest
docker pull openvulcan/vldb-duckdb:latest
```

```bash
docker run -d \
  --name vldb-lancedb \
  --restart unless-stopped \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  openvulcan/vldb-lancedb:latest

docker run -d \
  --name vldb-duckdb \
  --restart unless-stopped \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  openvulcan/vldb-duckdb:latest
```

Default endpoints:

- `vldb-lancedb`: `127.0.0.1:50051`
- `vldb-duckdb`: `127.0.0.1:50052`

Detailed Docker install guides:

- English: [docs/docker-install.en.md](./docs/docker-install.en.md)
- Chinese: [docs/docker-install.zh-CN.md](./docs/docker-install.zh-CN.md)

### Prebuilt Native Binaries

If you prefer not to use Docker, download the matching release archive from GitHub Releases, extract it, copy the example config files, and run the binaries directly.

Typical startup commands:

```bash
./vldb-lancedb --config ./vldb-lancedb.json
./vldb-duckdb --config ./vldb-duckdb.json
```

Detailed binary install guides:

- English: [docs/native-install.en.md](./docs/native-install.en.md)
- Chinese: [docs/native-install.zh-CN.md](./docs/native-install.zh-CN.md)

## Developer Setup

### Build From Source

```bash
cd ./vldb-lancedb
cargo build

cd ../vldb-duckdb
cargo build
```

### Build Docker Images Locally

For local image builds, Dockerfiles, compose-based development, and custom Docker configs, see the developer Docker guide:

- English: [docs/docker.en.md](./docs/docker.en.md)
- Chinese: [docs/docker.zh-CN.md](./docs/docker.zh-CN.md)

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
- Native binary install guide:
  - English: [docs/native-install.en.md](./docs/native-install.en.md)
  - Chinese: [docs/native-install.zh-CN.md](./docs/native-install.zh-CN.md)
- Docker quick install guide:
  - English: [docs/docker-install.en.md](./docs/docker-install.en.md)
  - Chinese: [docs/docker-install.zh-CN.md](./docs/docker-install.zh-CN.md)
- Docker build guide for developers:
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
