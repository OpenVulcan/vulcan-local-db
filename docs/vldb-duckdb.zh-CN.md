# vldb-duckdb 使用说明

## 项目是什么

`vldb-duckdb` 是一个基于 Rust 和 gRPC 的 DuckDB 微服务。它把本地 DuckDB 数据库封装成三个核心 RPC：

- `ExecuteScript`：执行 DDL、DML 或单条参数化 SQL，不返回结果集
- `QueryJson`：执行轻量查询，并把结果直接转成 JSON 字符串返回
- `QueryStream`：执行查询，并把结果以 Arrow IPC 流式字节块返回

适合场景：

- 需要在本地以服务方式封装 DuckDB
- 需要安全地做参数化 SQL 写入，避免直接拼接字符串
- 需要快速获取 `count(*)`、状态值等轻量 JSON 结果
- 需要把查询结果交给下游 Arrow 客户端处理
- 需要从其他语言通过 gRPC 调用 SQL 执行能力

## 目录与关键文件

- 工程目录：`vldb-duckdb/`
- 配置示例：`vldb-duckdb/vldb-duckdb.json.example`
- 服务入口：`vldb-duckdb/src/main.rs`
- 配置加载：`vldb-duckdb/src/config.rs`
- gRPC 协议：`vldb-duckdb/proto/v1/duckdb.proto`
- Go 示例：`vldb-duckdb/demo/go-client/`

## 环境要求

- Rust `1.94.0`
- Go `1.24+`，仅在需要运行 Go 示例时使用

## 如何构建

在项目根目录执行：

```bash
cd ./vldb-duckdb
cargo build
cargo build --release
```

## 如何启动

1. 准备配置文件：

```bash
cd ./vldb-duckdb
copy .\\vldb-duckdb.json.example .\\vldb-duckdb.json
```

2. 启动服务：

```bash
cargo run --release -- --config .\\vldb-duckdb.json
```

如果不显式传 `--config`，服务会自动按配置发现顺序查找。

## Docker 安装与启动

构建镜像：

```bash
docker build -t vulcan/vldb-duckdb:local ./vldb-duckdb
```

启动容器：

```bash
docker run -d \
  --name vldb-duckdb \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  -v ./vldb-duckdb/docker/vldb-duckdb.json:/app/config/vldb-duckdb.json:ro \
  vulcan/vldb-duckdb:local
```

说明：

- 镜像默认使用 `vldb-duckdb/docker/vldb-duckdb.json`
- 这个 Docker 配置使用 `db_path: "/app/data/duckdb.db"`，会直接写入挂载的 `/app/data`
- DuckDB 数据会写入容器内 `/app/data`

## 如何配置

默认配置示例：

```json
{
  "host": "0.0.0.0",
  "port": 50052,
  "db_path": "./data/duckdb.db",
  "memory_limit": "2GB",
  "threads": 4,
  "hardening": {
    "enforce_db_file_lock": true,
    "enable_external_access": false,
    "allowed_directories": [],
    "allowed_paths": [],
    "allow_community_extensions": false,
    "autoload_known_extensions": false,
    "autoinstall_known_extensions": false,
    "lock_configuration": true,
    "checkpoint_on_shutdown": true
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
    "log_file_name": "vldb-duckdb.log"
  }
}
```

字段说明：

- `host`：gRPC 监听地址
- `port`：gRPC 监听端口
- `db_path`：DuckDB 数据库文件路径
- `memory_limit`：DuckDB `PRAGMA memory_limit`
- `threads`：DuckDB `PRAGMA threads`
- `hardening.enforce_db_file_lock`：是否在数据库文件旁边持有一个进程级锁文件，防止另一个 `vldb-duckdb` 进程误用同一数据库
- `hardening.enable_external_access`：是否允许 SQL 访问数据库文件之外的本地/远程文件；默认关闭
- `hardening.allowed_directories`：在关闭外部访问时，仍允许访问的目录白名单
- `hardening.allowed_paths`：在关闭外部访问时，仍允许访问的文件白名单
- `hardening.allow_community_extensions`：是否允许社区扩展
- `hardening.autoload_known_extensions`：是否允许自动加载已知扩展
- `hardening.autoinstall_known_extensions`：是否允许自动安装已知扩展
- `hardening.lock_configuration`：启动后是否锁定 DuckDB 配置，阻止会话内再修改这些安全设置
- `hardening.checkpoint_on_shutdown`：正常退出时是否执行 checkpoint，减少 WAL 残留
- `logging.enabled`：服务日志总开关
- `logging.file_enabled`：是否写入日志文件
- `logging.stderr_enabled`：是否同时输出到标准错误
- `logging.request_log_enabled`：是否记录每次请求的开始、成功和失败日志
- `logging.slow_query_log_enabled`：是否记录慢 SQL 日志
- `logging.slow_query_threshold_ms`：慢 SQL 阈值，单位毫秒，默认 `1000`
- `logging.slow_query_full_sql_enabled`：慢 SQL 日志是否输出完整 SQL
- `logging.sql_preview_chars`：普通请求日志里的 SQL 预览最大长度
- `logging.log_dir`：可选自定义日志目录；为空时会自动使用 DuckDB 文件同级、带 `_log` 后缀的目录
- `logging.log_file_name`：日志基础文件名；服务会在扩展名前自动追加本地日期

配置发现顺序：

1. `--config <path>` 或 `-config <path>`
2. 运行目录下的 `vldb-duckdb.json`
3. 可执行文件所在目录下的 `vldb-duckdb.json`
4. 内置默认值

路径规则：

- 配置文件中的相对 `db_path` 以配置文件所在目录为基准计算
- 支持绝对路径
- 支持 `~`
- `logging.log_dir` 为空时，`./data/duckdb.db` 会把日志写到 `./data/duckdb_log/`
- 每天的日志文件会按 `vldb-duckdb_YYYY-MM-DD.log` 这样的形式分离
- `hardening.allowed_directories` 和 `hardening.allowed_paths` 里的相对路径也会按配置文件目录展开

## 如何调用函数

gRPC 服务名：

- `vldb.duckdb.v1.DuckDbService`

### 1. ExecuteScript

用途：

- 执行建表、删表、插入、更新等脚本
- 不返回行数据

请求：

- `sql: string`
- `params_json: string`

示例 SQL：

```sql
drop table if exists demo_items;
create table demo_items(id integer, name varchar, active boolean);
insert into demo_items values
  (1, 'alpha', true),
  (2, 'beta', true),
  (3, 'gamma', false);
```

返回：

- `success`
- `message`

参数说明：

- `params_json` 为空时，可执行多语句脚本
- `params_json` 不为空时，只支持单条 SQL 语句
- `params_json` 需要是 JSON 数组，例如 `[1, "alpha", true]`

### 2. QueryJson

用途：

- 执行简单查询
- 直接返回 JSON 字符串，适合计数、状态、轻量结果集

请求：

- `sql: string`
- `params_json: string`

返回：

- `json_data: string`

说明：

- 返回内容是 JSON 数组字符串
- 例如 `SELECT count(*) AS total FROM demo_items` 会返回类似 `[{"total":3}]`

### 3. QueryStream

用途：

- 执行查询
- 结果通过 `stream QueryResponse` 返回

请求：

- `sql: string`
- `params_json: string`

返回：

- `arrow_ipc_chunk: bytes`

客户端需要把所有 `arrow_ipc_chunk` 拼接起来，再按 Arrow IPC Stream 解码。

## Go 示例客户端

生成 Go stubs：

```bash
cd ./vldb-duckdb/demo/go-client
go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.11
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.6.1
./generate.sh
```

运行示例：

```bash
go run . -addr 127.0.0.1:50052 -out ./query.arrow.stream
```

示例客户端会：

1. 调用 `ExecuteScript` 创建表
2. 使用 `params_json` 做参数化插入
3. 调用 `QueryJson` 获取 `count(*)` 结果
4. 调用 `QueryStream` 获取 Arrow 数据流
5. 写出 `.arrow.stream` 文件
6. 再次读取并打印 Arrow 批次信息

## 注意事项

- `ExecuteScript` 适合无结果集脚本，不用于返回查询行
- `params_json` 当前只支持标量 JSON 值：`null`、布尔、数字、字符串
- `QueryJson` 适合轻量结果；大结果集仍建议使用 `QueryStream`
- `QueryStream` 的结果是 Arrow IPC 字节流，不是 JSON
- 服务内部会保持单个共享 DuckDB 连接，并通过这个连接串行执行请求
- 默认会在数据库文件旁边创建一个锁文件，例如 `duckdb.db.vldb.lock`，避免第二个 `vldb-duckdb` 进程悄悄复用同一个数据库文件
- `memory_limit` 和 `threads` 会在共享连接启动时应用
- 默认安全加固会关闭外部文件访问、社区扩展、已知扩展的自动安装/自动加载，并锁定配置。如果确实需要 `read_csv`、`COPY`、`ATTACH` 或扩展工作流，再通过 `hardening` 显式放开
- 超时日志和慢 SQL 日志现在会带上最后执行阶段，例如 `waiting_for_connection`、`acquiring_connection_lock`、`preparing_statement`、`executing_query`、`fetching_rows`、`serializing_json`
- 默认启用请求日志和慢 SQL 日志
- 当前服务没有内置认证、鉴权和 TLS
- 如果客户端不消费完整流，查询结果可能只部分到达
- 如果写请求收到 `ABORTED`，要把它理解成“提交结果不确定，先查库再决定是否重试”，不要直接盲重试

## 数据结构风险清单

- 不要用 `SELECT MAX(id) + 1` 分配主键。推荐 `CREATE SEQUENCE ...` + `DEFAULT nextval('...')`，或者直接用 UUID 主键。
- DuckDB 会为 `PRIMARY KEY`、`UNIQUE`、`FOREIGN KEY` 自动创建 ART 索引。它们能保证约束，但也会放大写入成本，并让更新约束列时更容易暴露冲突。
- 尽量把主键和唯一业务键设计成不可变。需要变更业务编码时，优先新写一列或做 copy-and-swap 迁移，不要原地批量更新主键。
- `VACUUM` 不会把数据库文件物理缩小到操作系统层面。大量删除或重写后，如果要真正回收磁盘空间，要配合 `CHECKPOINT`，或者导出/重建数据库文件。
- 重索引、补约束、重命名或重写重度索引表时，优先采用“新表建好 -> 回填 -> 切换”的迁移方式，风险比直接在老表上做复杂 `ALTER TABLE` 更低。
- 如果表里需要时间列，尽量统一成 UTC 语义；只有在明确需要会话时区语义时才使用 `TIMESTAMPTZ`，否则不同客户端时区可能看到不一致的日期边界。
