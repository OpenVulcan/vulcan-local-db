# Native Binary Install Guide

## Overview

If you prefer to run VulcanLocalDB without Docker, use the prebuilt release archives published on GitHub Releases:

- [OpenVulcan/vulcan-local-db Releases](https://github.com/OpenVulcan/vulcan-local-db/releases)

Each release provides platform-specific archives for:

- `vldb-lancedb`
- `vldb-duckdb`

Typical archive formats:

- Linux and macOS: `.tar.gz`
- Windows: `.zip`

## What To Download

Pick the archive that matches both your operating system and CPU architecture.

Common targets:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Examples:

- `vldb-lancedb-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- `vldb-duckdb-v0.1.0-x86_64-pc-windows-msvc.zip`

## Package Contents

Each archive contains:

- the service binary
- the example config file for that service
- `README.md`
- `LICENSE`

## Install Steps

### 1. Download And Extract

Linux or macOS:

```bash
tar -xzf vldb-lancedb-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
tar -xzf vldb-duckdb-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

Windows PowerShell:

```powershell
Expand-Archive .\vldb-lancedb-v0.1.0-x86_64-pc-windows-msvc.zip -DestinationPath .\vldb-lancedb
Expand-Archive .\vldb-duckdb-v0.1.0-x86_64-pc-windows-msvc.zip -DestinationPath .\vldb-duckdb
```

### 2. Prepare Config Files

Edit the shipped JSON config files before starting the services.

Typical defaults:

`vldb-lancedb`

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

`vldb-duckdb`

```json
{
  "host": "0.0.0.0",
  "port": 50052,
  "db_path": "./data/duckdb.db",
  "memory_limit": "2GB",
  "threads": 4,
  "logging": {
    "enabled": true,
    "file_enabled": true,
    "stderr_enabled": true,
    "request_log_enabled": true,
    "slow_query_log_enabled": true,
    "slow_query_threshold_ms": 1000,
    "slow_query_full_sql_enabled": true,
    "sql_preview_chars": 160,
    "log_dir": "",
    "log_file_name": "vldb-duckdb.log"
  }
}
```

Relative paths inside the config are resolved against the config file directory.

## Start The Services

Linux or macOS:

```bash
./vldb-lancedb --config ./vldb-lancedb.json
./vldb-duckdb --config ./vldb-duckdb.json
```

Windows PowerShell:

```powershell
.\vldb-lancedb.exe --config .\vldb-lancedb.json
.\vldb-duckdb.exe --config .\vldb-duckdb.json
```

Default endpoints:

- `vldb-lancedb`: `127.0.0.1:50051`
- `vldb-duckdb`: `127.0.0.1:50052`

## Verify The Services

Check that the process is listening on the expected port, then connect with your gRPC client.

The repository already includes Go demo clients:

- `vldb-lancedb/examples/go-client/`
- `vldb-duckdb/demo/go-client/`

Detailed service usage:

- [vldb-lancedb.en.md](./vldb-lancedb.en.md)
- [vldb-duckdb.en.md](./vldb-duckdb.en.md)

## Notes

- `vldb-lancedb` may require `protoc` when you build from source, but prebuilt release archives do not.
- `vldb-duckdb` exposes both `QueryJson` and `QueryStream`.
- For production deployments, prefer a stable absolute path for `db_path`.
- If you want containerized deployment instead, use [docker-install.en.md](./docker-install.en.md).
