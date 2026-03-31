# vldb-lancedb 使用说明

## 项目是什么

`vldb-lancedb` 是一个基于 Rust 和 gRPC 的 LanceDB 向量数据服务，主要提供五类能力：

- 建表：定义普通字段和向量字段
- 写入：支持 JSON 行数据或 Arrow IPC 数据写入
- 检索：执行向量相似度搜索，并返回 JSON 或 Arrow IPC
- 删除：按条件删除一批记录
- 删表：直接移除整张向量表

适合场景：

- 需要把 LanceDB 作为本地向量服务使用
- 需要用 gRPC 从其他语言调用向量写入和检索
- 需要按表方式管理向量字段和元数据字段

## 目录与关键文件

- 工程目录：`vldb-lancedb/`
- 配置示例：`vldb-lancedb/vldb-lancedb.json.example`
- 服务入口：`vldb-lancedb/src/main.rs`
- 配置加载：`vldb-lancedb/src/config.rs`
- gRPC 协议：`vldb-lancedb/proto/v1/lancedb.proto`
- Go 示例：`vldb-lancedb/examples/go-client/`

## 环境要求

- Rust `1.94.0`
- Go `1.24+`，仅在需要运行 Go 示例时使用
- `protoc`

说明：

- `vldb-lancedb` 依赖的 `lance-*` crate 在编译阶段会使用 `protoc`
- 如果系统没有 `protoc`，需要先安装，或设置 `PROTOC` 环境变量

## 如何构建

```bash
cd ./vldb-lancedb
cargo build
cargo build --release
```

## 如何启动

1. 准备配置文件：

```bash
cd ./vldb-lancedb
copy .\\vldb-lancedb.json.example .\\vldb-lancedb.json
```

2. 启动服务：

```bash
cargo run --release -- --config .\\vldb-lancedb.json
```

## Docker 安装与启动

构建镜像：

```bash
docker build -t vulcan/vldb-lancedb:local ./vldb-lancedb
```

启动容器：

```bash
docker run -d \
  --name vldb-lancedb \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  -v ./vldb-lancedb/docker/vldb-lancedb.json:/app/config/vldb-lancedb.json:ro \
  vulcan/vldb-lancedb:local
```

说明：

- 镜像默认使用 `vldb-lancedb/docker/vldb-lancedb.json`
- 这个 Docker 配置会监听 `0.0.0.0`
- 这个 Docker 配置使用 `db_path: "/app/data"`，会直接写入挂载的 `/app/data` 根目录
- LanceDB 数据会写入容器内 `/app/data`
- 在 Windows 的 Docker Desktop 环境下，推荐优先使用 Docker 命名卷做持久化

## 如何配置

默认配置示例：

```json
{
  "host": "127.0.0.1",
  "port": 50051,
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

字段说明：

- `host`：gRPC 监听地址
- `port`：gRPC 监听端口
- `db_path`：LanceDB 数据目录或远程 URI
- `read_consistency_interval_ms`：读取其他进程写入的刷新间隔；默认 `0` 表示每次读都检查最新版本，非 `0` 表示最终一致，设为 `null` 表示关闭跨进程刷新检查
- `logging.enabled`：服务日志总开关
- `logging.file_enabled`：是否写入日志文件
- `logging.stderr_enabled`：是否同时输出到标准错误
- `logging.request_log_enabled`：是否记录每次请求的开始、成功和失败日志
- `logging.slow_request_log_enabled`：是否记录慢请求日志
- `logging.slow_request_threshold_ms`：慢请求阈值，单位毫秒，默认 `1000`
- `logging.include_request_details_in_slow_log`：慢请求日志是否带上请求摘要
- `logging.request_preview_chars`：请求摘要预览最大长度
- `logging.log_dir`：可选自定义日志目录；为空且 `db_path` 为本地目录时，默认使用 `<db_path>/logs/`
- `logging.log_file_name`：日志基础文件名；服务会在扩展名前自动追加本地日期

配置发现顺序：

1. `--config <path>` 或 `-config <path>`
2. 可执行文件目录下的 `vldb-lancedb.json`
3. 可执行文件目录下的 `lancedb.json`
4. 运行目录下的 `vldb-lancedb.json`
5. 运行目录下的 `lancedb.json`
6. 内置默认值

路径规则：

- 如果 `db_path` 是本地相对路径，会以配置文件所在目录为基准解析
- 如果 `db_path` 看起来像 URI，例如包含 `://`，则直接按原值使用
- 本地目录不存在时，服务会自动创建
- `logging.log_dir` 为空且 `db_path` 为本地目录时，日志默认写入 `<db_path>/logs/`
- 每天的日志文件会按 `vldb-lancedb_YYYY-MM-DD.log` 这样的形式分离
- 服务会按表名协调并发访问：同一张表的写操作会串行执行，查询会与同表的删表或覆盖写互斥，以降低提交冲突和元数据竞争
- 如果 `db_path` 使用普通 `s3://`，服务启动时会给出告警；当存在多写入者或多实例时，应改用 `s3+ddb://`

## 如何调用函数

gRPC 服务名：

- `vldb.lancedb.v1.LanceDbService`

### 1. CreateTable

用途：

- 创建新表
- 定义普通字段和向量字段

关键字段：

- `table_name`
- `columns`
- `overwrite_if_exists`

向量字段注意：

- `column_type` 需要使用 `COLUMN_TYPE_VECTOR_FLOAT32`
- `vector_dim` 必须大于 `0`

### 2. VectorUpsert

用途：

- 写入向量数据
- 支持追加或按主键列合并

关键字段：

- `table_name`
- `input_format`
- `data`
- `key_columns`

输入格式：

- `INPUT_FORMAT_JSON_ROWS`
- `INPUT_FORMAT_ARROW_IPC`

行为说明：

- `key_columns` 为空时，执行追加写入
- `key_columns` 非空时，执行 merge upsert

JSON 写入要求：

- `data` 需要是 JSON 数组
- 每一行需要是 JSON 对象
- 向量字段需要是浮点数组
- 非空字段不能缺失

### 3. VectorSearch

用途：

- 执行向量相似度检索

关键字段：

- `table_name`
- `vector`
- `limit`
- `filter`
- `vector_column`
- `output_format`

输出格式：

- `OUTPUT_FORMAT_JSON_ROWS`
- `OUTPUT_FORMAT_ARROW_IPC`

说明：

- `vector` 不能为空
- `limit=0` 时服务内部会回退到默认值 `10`
- `filter` 可用于附加条件，例如 `active = true`

### 4. Delete

用途：

- 按条件删除表中的记录
- 适合实现会话遗忘、清理历史数据等场景

关键字段：

- `table_name`
- `condition`

条件示例：

- `session_id = 'abc'`
- `id >= 100`

返回：

- `success`
- `message`
- `version`
- `deleted_rows`

### 5. DropTable

用途：

- 删除整张 LanceDB 表

关键字段：

- `table_name`

返回：

- `success`
- `message`

## Go 示例客户端

生成 Go stubs：

```bash
go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.11
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.6.1

protoc \
  -I . \
  --go_out=./examples/go-client/gen \
  --go_opt=paths=source_relative \
  --go-grpc_out=./examples/go-client/gen \
  --go-grpc_opt=paths=source_relative \
  ./proto/v1/lancedb.proto
```

运行 Go 示例：

```bash
cd ./examples/go-client
go mod tidy
go run .
```

示例会顺序执行：

1. `CreateTable`
2. `VectorUpsert`
3. `VectorSearch`
4. `Delete`
5. `DropTable`

## 注意事项

- 当前服务没有内置认证、鉴权和 TLS
- 向量字段当前按 `float32` 固定长度列表处理
- JSON 导入时字段类型必须与表结构匹配
- `VectorSearch` 返回 JSON 时，结果中可能包含 `_distance`
- `Delete.condition` 会直接传给 LanceDB 作为过滤表达式，调用方需要自己保证条件字符串正确
- 默认启用请求日志和慢请求日志
- 编译环境缺少 `protoc` 时，Rust 构建会失败
