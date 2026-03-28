# Docker Guide

## Overview

This repository now includes Docker packaging files for both services:

- `vldb-lancedb/Dockerfile`
- `vldb-duckdb/Dockerfile`
- root-level `docker-compose.example.yml`

Both images use Docker-specific config files and store service data under `/app/data`.
The application still resolves relative `db_path` values against the config file directory, but the Docker configs use absolute `/app/data/...` paths so the mounted volume is unambiguous.

## Prerequisites

- Docker installed
- Docker Compose v2 if you want to start both services together

## 1. Build A Single Image

### Build vldb-lancedb

```bash
docker build -t vulcan/vldb-lancedb:local ./vldb-lancedb
```

### Build vldb-duckdb

```bash
docker build -t vulcan/vldb-duckdb:local ./vldb-duckdb
```

## 2. Run A Single Container

### Run vldb-lancedb

```bash
docker run -d \
  --name vldb-lancedb \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  -v ./vldb-lancedb/docker/vldb-lancedb.json:/app/config/vldb-lancedb.json:ro \
  vulcan/vldb-lancedb:local
```

### Run vldb-duckdb

```bash
docker run -d \
  --name vldb-duckdb \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  -v ./vldb-duckdb/docker/vldb-duckdb.json:/app/config/vldb-duckdb.json:ro \
  vulcan/vldb-duckdb:local
```

## 3. Start Both Services With Compose

```bash
docker compose -f ./docker-compose.example.yml up -d --build
```

Stop:

```bash
docker compose -f ./docker-compose.example.yml down
```

## 4. Configuration

The containers use these default config files:

- `vldb-lancedb/docker/vldb-lancedb.json`
- `vldb-duckdb/docker/vldb-duckdb.json`

Notes:

- `vldb-lancedb` uses `0.0.0.0` inside the container instead of `127.0.0.1`
- `vldb-duckdb` also listens on `0.0.0.0`
- `vldb-lancedb/docker/vldb-lancedb.json` uses `/app/data/lancedb`
- `vldb-duckdb/docker/vldb-duckdb.json` uses `/app/data/duckdb.db`
- both services write their data under `/app/data`

If you need custom settings, edit the mounted JSON files or replace the bind mounts with your own files.

### Replace The Config File

If you want to use your own config file, there are two common patterns.

Option 1: replace the default mounted config path directly.

```bash
docker run -d \
  --name vldb-duckdb \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  -v ./configs/my-duckdb.json:/app/config/vldb-duckdb.json:ro \
  vulcan/vldb-duckdb:local
```

Option 2: mount the config somewhere else and pass `--config`.

```bash
docker run -d \
  --name vldb-duckdb \
  -p 50060:50060 \
  -v vldb-duckdb-data:/app/data \
  -v ./configs/my-duckdb.json:/app/config/custom.json:ro \
  vulcan/vldb-duckdb:local \
  --config /app/config/custom.json
```

The same pattern works for `vldb-lancedb`; just change the file name to `vldb-lancedb.json` or point `--config` at the desired path.

## 5. Ports And Data Paths

- `vldb-lancedb`: container port `50051`
- `vldb-duckdb`: container port `50052`
- `vldb-lancedb` data path: `/app/data`
- `vldb-duckdb` data path: `/app/data`

Recommended persistent volumes:

- `vldb-lancedb-data`
- `vldb-duckdb-data`

### Change Ports

If you only want to change the host-side port and keep the container listener unchanged, update only the left side of `-p`:

```bash
-p 60052:50052
```

That keeps the service listening on `50052` inside the container while exposing `60052` on the host.

If you want to change the in-container listening port as well, update both:

- the `port` value in the config file
- the port mapping in `docker run` or compose

Example for `vldb-duckdb` on `50060`:

```json
{
  "host": "0.0.0.0",
  "port": 50060,
  "db_path": "/app/data/duckdb.db",
  "memory_limit": "2GB",
  "threads": 4
}
```

```bash
-p 50060:50060
```

## 6. Multi-Instance Deployment

The current images follow a "one container, one process, one port" model.  
If you want the same image to serve multiple ports, the recommended setup is multiple containers, each with:

- its own container name
- its own config file
- its own host port
- its own data volume or its own `db_path`

Example compose layout for `vldb-duckdb`:

```yaml
services:
  vldb-duckdb-50052:
    image: vulcan/vldb-duckdb:local
    container_name: vldb-duckdb-50052
    ports:
      - "50052:50052"
    volumes:
      - vldb-duckdb-data-50052:/app/data
      - ./configs/vldb-duckdb-50052.json:/app/config/vldb-duckdb.json:ro

  vldb-duckdb-50062:
    image: vulcan/vldb-duckdb:local
    container_name: vldb-duckdb-50062
    ports:
      - "50062:50062"
    volumes:
      - vldb-duckdb-data-50062:/app/data
      - ./configs/vldb-duckdb-50062.json:/app/config/vldb-duckdb.json:ro

volumes:
  vldb-duckdb-data-50052:
  vldb-duckdb-data-50062:
```

The same pattern applies to `vldb-lancedb`.  
Do not let multiple instances share the same LanceDB directory or the same DuckDB database file.

## 7. Current Compose Grouping

The repository-level [docker-compose.example.yml](../docker-compose.example.yml) is already grouped by service:

- one service block for `vldb-lancedb`
- one service block for `vldb-duckdb`
- one mounted config file per service
- one named data volume per service

The current default grouping is:

- `vldb-lancedb` -> `vldb-lancedb-data`
- `vldb-duckdb` -> `vldb-duckdb-data`

If you expand to multiple instances later, keep the same grouping pattern:

- group by service type
- split config files by port or instance name
- split data volumes per instance

## 8. Notes

- the original `vldb-lancedb` example config listens on `127.0.0.1`, which is not suitable for containers, so the image uses a Docker-specific config
- if you only need one service, you can build and run its project-local Dockerfile directly
- on Docker Desktop for Windows, `vldb-lancedb` is verified with a Docker named volume; host bind mounts can trigger LanceDB metadata I/O issues
- verified on March 26, 2026 with live `docker build`, `docker compose up`, and the existing Go demo clients
