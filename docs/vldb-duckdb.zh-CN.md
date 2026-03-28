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
  "threads": 4
}
```

字段说明：

- `host`：gRPC 监听地址
- `port`：gRPC 监听端口
- `db_path`：DuckDB 数据库文件路径
- `memory_limit`：DuckDB `PRAGMA memory_limit`
- `threads`：DuckDB `PRAGMA threads`

配置发现顺序：

1. `--config <path>` 或 `-config <path>`
2. 运行目录下的 `vldb-duckdb.json`
3. 可执行文件所在目录下的 `vldb-duckdb.json`
4. 内置默认值

路径规则：

- 配置文件中的相对 `db_path` 以配置文件所在目录为基准计算
- 支持绝对路径
- 支持 `~`

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
- 服务内部会为每个请求克隆独立 DuckDB 连接
- `memory_limit` 和 `threads` 会在启动和请求连接上重复应用
- 当前服务没有内置认证、鉴权和 TLS
- 如果客户端不消费完整流，查询结果可能只部分到达
