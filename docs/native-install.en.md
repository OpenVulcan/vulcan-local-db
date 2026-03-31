# Native Binary Install Guide

## Overview

The service release archives now live with each child project. Download the matching archive from the service repository you want to run:

- `vldb-lancedb`
- `vldb-sqlite`

Typical archive formats:

- Linux and macOS: `.tar.gz`
- Windows: `.zip`

## What To Download

Pick the archive that matches both your operating system and CPU architecture.

Common targets:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Examples:

- `vldb-lancedb-v0.1.1-x86_64-unknown-linux-gnu.tar.gz`
- `vldb-sqlite-v0.1.0-x86_64-pc-windows-msvc.zip`

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
tar -xzf vldb-lancedb-v0.1.1-x86_64-unknown-linux-gnu.tar.gz
tar -xzf vldb-sqlite-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

Windows PowerShell:

```powershell
Expand-Archive .\vldb-lancedb-v0.1.1-x86_64-pc-windows-msvc.zip -DestinationPath .\vldb-lancedb
Expand-Archive .\vldb-sqlite-v0.1.0-x86_64-pc-windows-msvc.zip -DestinationPath .\vldb-sqlite
```

### 2. Prepare Config Files

Edit the shipped JSON config files before starting the services.

Typical defaults:

`vldb-lancedb`

```json
{
  "host": "127.0.0.1",
  "port": 19301,
  "db_path": "./data",
  "read_consistency_interval_ms": 0,
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

`vldb-sqlite`

```json
{
  "host": "0.0.0.0",
  "port": 19501,
  "db_path": "./data/sqlite.db",
  "connection_pool_size": 8,
  "busy_timeout_ms": 5000,
  "pragmas": {
    "journal_mode": "WAL",
    "synchronous": "NORMAL",
    "foreign_keys": true
  },
  "hardening": {
    "enforce_db_file_lock": true,
    "read_only": false,
    "allow_uri_filenames": false,
    "trusted_schema": false,
    "defensive": true
  },
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
    "log_file_name": "vldb-sqlite.log"
  }
}
```

Relative paths inside the config are resolved against the config file directory.

## Start The Services

Linux or macOS:

```bash
./vldb-lancedb --config ./vldb-lancedb.json
./vldb-sqlite --config ./vldb-sqlite.json
```

Windows PowerShell:

```powershell
.\vldb-lancedb.exe --config .\vldb-lancedb.json
.\vldb-sqlite.exe --config .\vldb-sqlite.json
```

Default endpoints:

- `vldb-lancedb`: `127.0.0.1:19301`
- `vldb-sqlite`: `127.0.0.1:19501`

## Verify The Services

Check that the process is listening on the expected port, then connect with your gRPC client.

Detailed service usage:

- [../vldb-lancedb/docs/README.en.md](../vldb-lancedb/docs/README.en.md)
- [../vldb-sqlite/docs/README.en.md](../vldb-sqlite/docs/README.en.md)

## Notes

- `vldb-lancedb` may require `protoc` when you build from source, but prebuilt release archives do not.
- `vldb-sqlite` exposes both `QueryJson` and `QueryStream`.
- SQLite release configs default to WAL mode.
- For production deployments, prefer a stable absolute path for `db_path`.
- If you want containerized deployment instead, use [docker-install.en.md](./docker-install.en.md).
