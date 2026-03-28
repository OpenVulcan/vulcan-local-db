# Docker Install Guide

## Overview

If you only want to install and run the published images, use the Docker Hub images below:

- `openvulcan/vldb-lancedb:latest`
- `openvulcan/vldb-duckdb:latest`

This guide uses direct `docker pull` and `docker run` commands instead of building from source.

## Prerequisites

- Docker installed
- network access to Docker Hub

## 1. Pull The Images

```bash
docker pull openvulcan/vldb-lancedb:latest
docker pull openvulcan/vldb-duckdb:latest
```

## 2. Run Each Service

### Start `vldb-lancedb`

```bash
docker run -d \
  --name vldb-lancedb \
  --restart unless-stopped \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  openvulcan/vldb-lancedb:latest
```

### Start `vldb-duckdb`

```bash
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

## 3. Check The Running Containers

```bash
docker ps
docker logs -f vldb-lancedb
docker logs -f vldb-duckdb
```

## 4. Stop Or Remove The Services

Stop:

```bash
docker stop vldb-lancedb vldb-duckdb
```

Remove:

```bash
docker rm vldb-lancedb vldb-duckdb
```

Remove volumes too, if you want to delete persisted data:

```bash
docker volume rm vldb-lancedb-data vldb-duckdb-data
```

## 5. Use A Custom Config File

If you want to override the default config, mount your own JSON file into the container.

### Custom config for `vldb-lancedb`

```bash
docker run -d \
  --name vldb-lancedb \
  --restart unless-stopped \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  -v /your/path/vldb-lancedb.json:/app/config/vldb-lancedb.json:ro \
  openvulcan/vldb-lancedb:latest
```

### Custom config for `vldb-duckdb`

```bash
docker run -d \
  --name vldb-duckdb \
  --restart unless-stopped \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  -v /your/path/vldb-duckdb.json:/app/config/vldb-duckdb.json:ro \
  openvulcan/vldb-duckdb:latest
```

## 6. Change Ports

If you only want to change the host-side port, change the left side of `-p`.

Example:

```bash
docker run -d \
  --name vldb-duckdb \
  --restart unless-stopped \
  -p 60052:50052 \
  -v vldb-duckdb-data:/app/data \
  openvulcan/vldb-duckdb:latest
```

That exposes the service at `127.0.0.1:60052` while the container still listens on `50052`.

If you want to change the in-container listening port too, mount a custom config file and update both:

- the `port` field in the JSON config
- the `-p host_port:container_port` mapping

## 7. Optional Compose Example

```yaml
name: vulcanlocaldb

services:
  vldb-lancedb:
    image: openvulcan/vldb-lancedb:latest
    container_name: vldb-lancedb
    restart: unless-stopped
    ports:
      - "50051:50051"
    volumes:
      - vldb-lancedb-data:/app/data

  vldb-duckdb:
    image: openvulcan/vldb-duckdb:latest
    container_name: vldb-duckdb
    restart: unless-stopped
    ports:
      - "50052:50052"
    volumes:
      - vldb-duckdb-data:/app/data

volumes:
  vldb-lancedb-data:
  vldb-duckdb-data:
```

Start it with:

```bash
docker compose up -d
```

## Notes

- These images expose gRPC services, not REST services.
- `vldb-lancedb` stores data directly under `/app/data`.
- `vldb-duckdb` stores data under `/app/data/duckdb.db`.
- Named volumes are the safest default for persistence.
