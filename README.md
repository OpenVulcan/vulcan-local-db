# VulcanLocalDB

English | [简体中文](./README.zh-CN.md)

VulcanLocalDB is a local-first data gateway workspace for applications and AI agents that need fast, structured access to private data without pushing everything into a remote service. It packages two Rust gRPC services and one Rust terminal manager:

- `vldb-lancedb`: a vector data gateway built on LanceDB
- `vldb-sqlite`: a SQLite-native SQL gateway for embedded local databases
- `vldb-manager`: a cross-platform `ratatui` control plane for building, starting, stopping, and monitoring the local gateways

Together they give you a simple local deployment model:

- store and search embeddings through LanceDB
- expose embedded SQLite databases through a gRPC boundary
- use lightweight JSON for small results and Arrow IPC for large result sets
- integrate from other languages through stable gRPC APIs

## What This Repository Contains

| Service | Purpose | Typical Use |
| --- | --- | --- |
| `vldb-lancedb` | Vector table management, vector upsert, nearest-neighbor search, conditional delete, and table drop | Agent memory, local RAG, semantic search, forgetting and cleanup |
| `vldb-sqlite` | SQLite-native parameterized SQL execution, JSON queries, and Arrow IPC streaming | Embedded app databases, local metadata stores, lightweight transactional SQL APIs |
| `vldb-manager` | Cross-platform terminal UI for config generation, builds, process control, and output inspection | Replace scattered shell / PowerShell control scripts with a unified Rust interface |

`vldb-lancedb` and `vldb-sqlite` now keep their own service-specific docs and release automation, while `vldb-manager` remains the local operator UI for the workspace.

## Why This Project Exists

This repository is designed for scenarios where you want a small local gateway instead of a large application server:

- desktop or edge deployments that need embedded data services
- AI assistants that need local vector memory and SQL access
- internal tools that prefer gRPC over direct database coupling
- services that want one path for lightweight JSON results and another for high-volume Arrow data

## Quick Install

### Script Installer From GitHub

If you want a guided native installation flow, use the installer scripts published from this repository.

Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.sh | bash
```

macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.ps1 | iex
```

Detailed script install guides:

- English: [docs/script-install.en.md](./docs/script-install.en.md)
- Chinese: [docs/script-install.zh-CN.md](./docs/script-install.zh-CN.md)

### Docker Hub

Pull and run the published images directly:

```bash
docker pull openvulcan/vldb-lancedb:latest
docker pull openvulcan/vldb-sqlite:latest
```

```bash
docker run -d \
  --name vldb-lancedb \
  --restart unless-stopped \
  -p 19301:19301 \
  -v vldb-lancedb-data:/app/data \
  openvulcan/vldb-lancedb:latest

docker run -d \
  --name vldb-sqlite \
  --restart unless-stopped \
  -p 19501:19501 \
  -v vldb-sqlite-data:/app/data \
  openvulcan/vldb-sqlite:latest
```

Default endpoints:

- `vldb-lancedb`: `127.0.0.1:19301`
- `vldb-sqlite`: `127.0.0.1:19501`

Detailed Docker install guides:

- English: [docs/docker-install.en.md](./docs/docker-install.en.md)
- Chinese: [docs/docker-install.zh-CN.md](./docs/docker-install.zh-CN.md)

### Prebuilt Native Binaries

If you prefer not to use Docker, download the matching release archive from GitHub Releases, extract it, copy the example config files, and run the binaries directly.

Typical startup commands:

```bash
./vldb-lancedb --config ./vldb-lancedb.json
./vldb-sqlite --config ./vldb-sqlite.json
```

Detailed binary install guides:

- English: [docs/native-install.en.md](./docs/native-install.en.md)
- Chinese: [docs/native-install.zh-CN.md](./docs/native-install.zh-CN.md)

## Developer Setup

### Build From Source

```bash
cd ./vldb-lancedb
cargo build

cd ../vldb-sqlite
cargo build

cd ../vldb-manager
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
|-- vldb-sqlite/
|-- vldb-manager/
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
- Script installer guide:
  - English: [docs/script-install.en.md](./docs/script-install.en.md)
  - Chinese: [docs/script-install.zh-CN.md](./docs/script-install.zh-CN.md)
- Script review and remediation note:
  - Chinese: [docs/VLDB_script_review.md](./docs/VLDB_script_review.md)
- Docker quick install guide:
  - English: [docs/docker-install.en.md](./docs/docker-install.en.md)
  - Chinese: [docs/docker-install.zh-CN.md](./docs/docker-install.zh-CN.md)
- Docker build guide for developers:
  - English: [docs/docker.en.md](./docs/docker.en.md)
  - Chinese: [docs/docker.zh-CN.md](./docs/docker.zh-CN.md)
- Service guides:
  - `vldb-lancedb`
    - README: [vldb-lancedb/README.md](./vldb-lancedb/README.md)
    - English: [vldb-lancedb/docs/README.en.md](./vldb-lancedb/docs/README.en.md)
    - Chinese: [vldb-lancedb/docs/README.zh-CN.md](./vldb-lancedb/docs/README.zh-CN.md)
  - `vldb-sqlite`
    - README: [vldb-sqlite/README.md](./vldb-sqlite/README.md)
    - English: [vldb-sqlite/docs/README.en.md](./vldb-sqlite/docs/README.en.md)
    - Chinese: [vldb-sqlite/docs/README.zh-CN.md](./vldb-sqlite/docs/README.zh-CN.md)
- Manager guide:
  - `vldb-manager`
    - README: [vldb-manager/README.md](./vldb-manager/README.md)

## Current Status

- `vldb-lancedb` and `vldb-sqlite` both build in `debug` and `release`
- `vldb-manager` builds as a Rust 2024 terminal UI with `ratatui`
- both gateway services have local Docker packaging
- service release pipelines now live in the child projects

## Notes

- this repository exposes gRPC services, not REST APIs
- `vldb-lancedb` requires `protoc` during Rust builds
- `vldb-sqlite` supports both `QueryJson` and `QueryStream`
- `vldb-sqlite` is tuned around SQLite PRAGMAs, WAL mode, locking, and dynamic typing
- on Docker Desktop for Windows, `vldb-lancedb` is best persisted with Docker named volumes

## License

This repository is distributed under the [LICENSE](./LICENSE).
