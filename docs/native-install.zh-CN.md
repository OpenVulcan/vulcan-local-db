# 二进制安装说明

## 说明

服务二进制发布已经下沉到各自子项目维护。你可以直接去对应服务仓库下载平台压缩包：

- `vldb-lancedb`
- `vldb-sqlite`

常见压缩格式：

- Linux 和 macOS：`.tar.gz`
- Windows：`.zip`

## 下载哪个文件

请选择与你的操作系统和 CPU 架构同时匹配的压缩包。

常见目标平台：

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

例如：

- `vldb-lancedb-v0.1.1-x86_64-unknown-linux-gnu.tar.gz`
- `vldb-sqlite-v0.1.0-x86_64-pc-windows-msvc.zip`

## 压缩包内容

每个压缩包通常包含：

- 服务可执行文件
- 对应服务的示例配置文件
- `README.md`
- `LICENSE`

## 安装步骤

### 1. 下载并解压

Linux 或 macOS：

```bash
tar -xzf vldb-lancedb-v0.1.1-x86_64-unknown-linux-gnu.tar.gz
tar -xzf vldb-sqlite-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

Windows PowerShell：

```powershell
Expand-Archive .\vldb-lancedb-v0.1.1-x86_64-pc-windows-msvc.zip -DestinationPath .\vldb-lancedb
Expand-Archive .\vldb-sqlite-v0.1.0-x86_64-pc-windows-msvc.zip -DestinationPath .\vldb-sqlite
```

### 2. 准备配置文件

启动前请先编辑压缩包里的 JSON 配置文件。

典型默认值如下：

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

配置文件中的相对路径会以配置文件所在目录作为基准来解析。

## 启动服务

Linux 或 macOS：

```bash
./vldb-lancedb --config ./vldb-lancedb.json
./vldb-sqlite --config ./vldb-sqlite.json
```

Windows PowerShell：

```powershell
.\vldb-lancedb.exe --config .\vldb-lancedb.json
.\vldb-sqlite.exe --config .\vldb-sqlite.json
```

默认访问地址：

- `vldb-lancedb`：`127.0.0.1:19301`
- `vldb-sqlite`：`127.0.0.1:19501`

## 验证服务

确认进程已经监听对应端口后，再用你的 gRPC 客户端连接。

详细接口与调用方式可继续参考：

- [OpenVulcan/vldb-lancedb](https://github.com/OpenVulcan/vldb-lancedb/blob/main/docs/README.zh-CN.md)
- [OpenVulcan/vldb-sqlite](https://github.com/OpenVulcan/vldb-sqlite/blob/main/docs/README.zh-CN.md)

## 注意事项

- `vldb-lancedb` 在源码编译时可能需要 `protoc`，但直接使用发布包时不需要。
- `vldb-sqlite` 同时提供 `QueryJson` 和 `QueryStream`。
- SQLite 默认配置启用了 WAL。
- 生产环境建议把 `db_path` 配成稳定的绝对路径。
- 如果你更希望用容器方式部署，请查看 [docker-install.zh-CN.md](./docker-install.zh-CN.md)。
