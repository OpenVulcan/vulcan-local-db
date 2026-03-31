# Docker Install Guide

## Overview

If you only want to install and run the published images, use the Docker Hub images below:

- `openvulcan/vldb-lancedb:latest`
- `openvulcan/vldb-sqlite:latest`

This guide uses direct `docker pull` and `docker run` commands instead of building from source.

## Prerequisites

- Docker installed
- network access to Docker Hub

## 1. Pull The Images

```bash
docker pull openvulcan/vldb-lancedb:latest
docker pull openvulcan/vldb-sqlite:latest
```

## 2. Run Each Service

### Start `vldb-lancedb`

```bash
docker run -d \
  --name vldb-lancedb \
  --restart unless-stopped \
  -p 19301:19301 \
  -v vldb-lancedb-data:/app/data \
  openvulcan/vldb-lancedb:latest
```

### Start `vldb-sqlite`

```bash
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

## 3. Check The Running Containers

```bash
docker ps
docker logs -f vldb-lancedb
docker logs -f vldb-sqlite
```

## 4. Stop Or Remove The Services

Stop:

```bash
docker stop vldb-lancedb vldb-sqlite
```

Remove:

```bash
docker rm vldb-lancedb vldb-sqlite
```

Remove volumes too, if you want to delete persisted data:

```bash
docker volume rm vldb-lancedb-data vldb-sqlite-data
```

## 5. Use A Custom Config File

If you want to override the default config, mount your own JSON file into the container.

### Custom config for `vldb-lancedb`

```bash
docker run -d \
  --name vldb-lancedb \
  --restart unless-stopped \
  -p 19301:19301 \
  -v vldb-lancedb-data:/app/data \
  -v /your/path/vldb-lancedb.json:/app/config/vldb-lancedb.json:ro \
  openvulcan/vldb-lancedb:latest
```

### Custom config for `vldb-sqlite`

```bash
docker run -d \
  --name vldb-sqlite \
  --restart unless-stopped \
  -p 19501:19501 \
  -v vldb-sqlite-data:/app/data \
  -v /your/path/vldb-sqlite.json:/app/config/vldb-sqlite.json:ro \
  openvulcan/vldb-sqlite:latest
```

## 6. Change Ports

If you only want to change the host-side port, change the left side of `-p`.

Example:

```bash
docker run -d \
  --name vldb-sqlite \
  --restart unless-stopped \
  -p 19601:19501 \
  -v vldb-sqlite-data:/app/data \
  openvulcan/vldb-sqlite:latest
```

That exposes the service at `127.0.0.1:19601` while the container still listens on `19501`.

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
      - "19301:19301"
    volumes:
      - vldb-lancedb-data:/app/data

  vldb-sqlite:
    image: openvulcan/vldb-sqlite:latest
    container_name: vldb-sqlite
    restart: unless-stopped
    ports:
      - "19501:19501"
    volumes:
      - vldb-sqlite-data:/app/data

volumes:
  vldb-lancedb-data:
  vldb-sqlite-data:
```

Start it with:

```bash
docker compose up -d
```

## Notes

- These images expose gRPC services, not REST services.
- `vldb-lancedb` stores data directly under `/app/data`.
- `vldb-sqlite` stores data under `/app/data/sqlite.db`.
- SQLite runs with WAL enabled by default in the published Docker config.
- Named volumes are the safest default for persistence.
