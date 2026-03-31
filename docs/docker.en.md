# Docker Build Guide

## Overview

This guide is for developers who want to build images locally, customize Dockerfiles, or run the workspace with local Docker build inputs.

The workspace keeps local Docker inputs for:

- `vldb-lancedb/Dockerfile`
- `vldb-sqlite/Dockerfile`
- root-level `docker-compose.example.yml`

Both images use Docker-specific config files and store service data under `/app/data`.
The application still resolves relative `db_path` values against the config file directory, but the Docker configs use absolute `/app/data/...` paths so the mounted volume is unambiguous.

Service release automation now lives in the child projects themselves; the root repository only keeps local developer examples.

If you want to install prebuilt Docker Hub images directly, see [docker-install.en.md](./docker-install.en.md).

## Prerequisites

- Docker installed
- Docker Compose v2 if you want to start both services together

## 1. Build A Single Image

### Build vldb-lancedb

```bash
docker build -t vulcan/vldb-lancedb:local ./vldb-lancedb
```

### Build vldb-sqlite

```bash
docker build -t vulcan/vldb-sqlite:local ./vldb-sqlite
```

## 2. Run A Single Container

### Run vldb-lancedb

```bash
docker run -d \
  --name vldb-lancedb \
  -p 19301:19301 \
  -v vldb-lancedb-data:/app/data \
  -v ./vldb-lancedb/docker/vldb-lancedb.json:/app/config/vldb-lancedb.json:ro \
  vulcan/vldb-lancedb:local
```

### Run vldb-sqlite

```bash
docker run -d \
  --name vldb-sqlite \
  -p 19501:19501 \
  -v vldb-sqlite-data:/app/data \
  -v ./vldb-sqlite/docker/vldb-sqlite.json:/app/config/vldb-sqlite.json:ro \
  vulcan/vldb-sqlite:local
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
- `vldb-sqlite/docker/vldb-sqlite.json`

Notes:

- `vldb-lancedb` uses `0.0.0.0` inside the container instead of `127.0.0.1`
- `vldb-sqlite` also listens on `0.0.0.0`
- `vldb-lancedb/docker/vldb-lancedb.json` uses `/app/data`
- `vldb-sqlite/docker/vldb-sqlite.json` uses `/app/data/sqlite.db`
- both services write their data under `/app/data`

If you need custom settings, edit the mounted JSON files or replace the bind mounts with your own files.

### Replace The Config File

If you want to use your own config file, there are two common patterns.

Option 1: replace the default mounted config path directly.

```bash
docker run -d \
  --name vldb-sqlite \
  -p 19501:19501 \
  -v vldb-sqlite-data:/app/data \
  -v ./configs/my-sqlite.json:/app/config/vldb-sqlite.json:ro \
  vulcan/vldb-sqlite:local
```

Option 2: mount the config somewhere else and pass `--config`.

```bash
docker run -d \
  --name vldb-sqlite \
  -p 19601:19601 \
  -v vldb-sqlite-data:/app/data \
  -v ./configs/my-sqlite.json:/app/config/custom.json:ro \
  vulcan/vldb-sqlite:local \
  --config /app/config/custom.json
```

The same pattern works for `vldb-lancedb`; just change the file name to `vldb-lancedb.json` or point `--config` at the desired path.

## 5. Ports And Data Paths

- `vldb-lancedb`: container port `19301`
- `vldb-sqlite`: container port `19501`
- `vldb-lancedb` data path: `/app/data`
- `vldb-sqlite` data path: `/app/data/sqlite.db`

Recommended persistent volumes:

- `vldb-lancedb-data`
- `vldb-sqlite-data`

### Change Ports

If you only want to change the host-side port and keep the container listener unchanged, update only the left side of `-p`:

```bash
-p 19601:19501
```

That keeps the service listening on `19501` inside the container while exposing `19601` on the host.

If you want to change the in-container listening port as well, update both:

- the `port` value in the config file
- the port mapping in `docker run` or compose

Example for `vldb-sqlite` on `19601`:

```json
{
  "host": "0.0.0.0",
  "port": 19601,
  "db_path": "/app/data/sqlite.db",
  "connection_pool_size": 8,
  "busy_timeout_ms": 5000,
  "pragmas": {
    "journal_mode": "WAL",
    "synchronous": "NORMAL",
    "foreign_keys": true
  }
}
```

```bash
-p 19601:19601
```

## 6. Multi-Instance Deployment

The current images follow a "one container, one process, one port" model.
If you want the same image to serve multiple ports, the recommended setup is multiple containers, each with:

- its own container name
- its own config file
- its own host port
- its own data volume or its own `db_path`

Example compose layout for `vldb-sqlite`:

```yaml
services:
  vldb-sqlite-19501:
    image: vulcan/vldb-sqlite:local
    container_name: vldb-sqlite-19501
    ports:
      - "19501:19501"
    volumes:
      - vldb-sqlite-data-19501:/app/data
      - ./configs/vldb-sqlite-19501.json:/app/config/vldb-sqlite.json:ro

  vldb-sqlite-19601:
    image: vulcan/vldb-sqlite:local
    container_name: vldb-sqlite-19601
    ports:
      - "19601:19601"
    volumes:
      - vldb-sqlite-data-19601:/app/data
      - ./configs/vldb-sqlite-19601.json:/app/config/vldb-sqlite.json:ro

volumes:
  vldb-sqlite-data-19501:
  vldb-sqlite-data-19601:
```

The same pattern applies to `vldb-lancedb`.
Do not let multiple instances share the same LanceDB directory or the same SQLite database file.

## 7. Current Compose Grouping

The repository-level [docker-compose.example.yml](../docker-compose.example.yml) is already grouped by service:

- one service block for `vldb-lancedb`
- one service block for `vldb-sqlite`
- one mounted config file per service
- one named data volume per service

The current default grouping is:

- `vldb-lancedb` -> `vldb-lancedb-data`
- `vldb-sqlite` -> `vldb-sqlite-data`

If you expand to multiple instances later, keep the same grouping pattern:

- group by service type
- split config files by port or instance name
- split data volumes per instance

## 8. Notes

- the original `vldb-lancedb` example config listens on `127.0.0.1`, which is not suitable for containers, so the image uses a Docker-specific config
- the published SQLite Docker config enables WAL by default
- if you only need one service, you can build and run its project-local Dockerfile directly
- on Docker Desktop for Windows, `vldb-lancedb` is verified with a Docker named volume; host bind mounts can trigger LanceDB metadata I/O issues
