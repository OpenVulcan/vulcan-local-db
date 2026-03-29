# VulcanLocalDB

[English](./README.md) | 简体中文

`VulcanLocalDB` 是一个面向本地部署场景的数据网关工作区，适合应用程序和 AI Agent 在不依赖远程中心化服务的情况下，统一访问本地向量数据与本地 SQL 数据。仓库当前包含两个 Rust gRPC 服务：

- `vldb-lancedb`：基于 LanceDB 的向量数据网关
- `vldb-duckdb`：基于 DuckDB 的 SQL 与分析数据网关

这两个服务组合起来，可以提供一套清晰的本地数据访问方式：

- 用 LanceDB 保存和检索向量数据
- 用 DuckDB 执行安全的参数化 SQL
- 小结果集走 JSON，大结果集走 Arrow IPC
- 通过稳定的 gRPC 接口供其他语言集成

## 仓库包含什么

| 服务 | 作用 | 典型用途 |
| --- | --- | --- |
| `vldb-lancedb` | 向量表管理、向量写入、近邻检索、条件删除、删表 | Agent Memory、本地 RAG、语义检索、遗忘与清理 |
| `vldb-duckdb` | 参数化 SQL 执行、轻量 JSON 查询、Arrow IPC 流式查询 | 本地分析、统计计数、表格查询接口、ETL 辅助 |

两个服务都带有 Go 示例客户端和 Docker 打包配置。

## 这个项目适合什么场景

这个仓库适合希望使用“本地网关”而不是“大型应用后端”的场景：

- 桌面端或边缘设备上的本地数据服务
- 需要本地向量记忆和 SQL 查询能力的 AI 助手
- 希望通过 gRPC 而不是直接绑定数据库的内部工具
- 同时需要轻量 JSON 结果和高吞吐 Arrow 数据流的系统

## 快速安装

### 直接使用 Docker Hub

如果你想最快速地安装并运行，直接拉取并启动已发布镜像：

```bash
docker pull openvulcan/vldb-lancedb:latest
docker pull openvulcan/vldb-duckdb:latest
```

```bash
docker run -d \
  --name vldb-lancedb \
  --restart unless-stopped \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  openvulcan/vldb-lancedb:latest

docker run -d \
  --name vldb-duckdb \
  --restart unless-stopped \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  openvulcan/vldb-duckdb:latest
```

默认访问地址：

- `vldb-lancedb`：`127.0.0.1:50051`
- `vldb-duckdb`：`127.0.0.1:50052`

详细 Docker 安装说明：

- 中文：[docs/docker-install.zh-CN.md](./docs/docker-install.zh-CN.md)
- English: [docs/docker-install.en.md](./docs/docker-install.en.md)

### 使用编译后的发布包

如果你不想使用 Docker，也可以直接下载 GitHub Releases 里的平台对应压缩包，解压后复制示例配置文件，再直接启动二进制。

典型启动命令：

```bash
./vldb-lancedb --config ./vldb-lancedb.json
./vldb-duckdb --config ./vldb-duckdb.json
```

详细二进制安装说明：

- 中文：[docs/native-install.zh-CN.md](./docs/native-install.zh-CN.md)
- English: [docs/native-install.en.md](./docs/native-install.en.md)

## 开发者使用

### 本地源码构建

```bash
cd ./vldb-lancedb
cargo build

cd ../vldb-duckdb
cargo build
```

### 本地构建 Docker 镜像

如果你是开发者，需要自行构建镜像、调试 Docker 配置或使用 compose 开发环境，请查看 Docker 构建说明：

- 中文：[docs/docker.zh-CN.md](./docs/docker.zh-CN.md)
- English: [docs/docker.en.md](./docs/docker.en.md)

## 仓库结构

```text
.
|-- vldb-lancedb/
|-- vldb-duckdb/
|-- docs/
|-- docker-compose.example.yml
|-- README.md
`-- README.zh-CN.md
```

## 文档导航

- 英文首页：[README.md](./README.md)
- 文档索引：[docs/README.zh-CN.md](./docs/README.zh-CN.md)
- 二进制安装说明：
  - 中文：[docs/native-install.zh-CN.md](./docs/native-install.zh-CN.md)
  - English: [docs/native-install.en.md](./docs/native-install.en.md)
- Docker 快速安装说明：
  - 中文：[docs/docker-install.zh-CN.md](./docs/docker-install.zh-CN.md)
  - English: [docs/docker-install.en.md](./docs/docker-install.en.md)
- Docker 构建说明：
  - 中文：[docs/docker.zh-CN.md](./docs/docker.zh-CN.md)
  - English: [docs/docker.en.md](./docs/docker.en.md)
- 服务说明：
  - `vldb-lancedb`
    - 中文：[docs/vldb-lancedb.zh-CN.md](./docs/vldb-lancedb.zh-CN.md)
    - English: [docs/vldb-lancedb.en.md](./docs/vldb-lancedb.en.md)
  - `vldb-duckdb`
    - 中文：[docs/vldb-duckdb.zh-CN.md](./docs/vldb-duckdb.zh-CN.md)
    - English: [docs/vldb-duckdb.en.md](./docs/vldb-duckdb.en.md)

## 当前状态

- 两个 Rust 服务都可以完成 `cargo build` 和 `cargo build --release`
- 两个 Go 示例客户端都可以构建并运行
- 两个服务都已经完成本地 gRPC 端到端 smoke test
- 两个服务都提供 Docker 打包与部署方式

## 补充说明

- 本仓库提供的是 gRPC 服务，不是 REST API
- `vldb-lancedb` 在 Rust 编译阶段依赖 `protoc`
- `vldb-duckdb` 同时支持 `QueryJson` 和 `QueryStream`
- 在 Windows 的 Docker Desktop 环境下，`vldb-lancedb` 推荐使用 Docker 命名卷持久化

## License

本仓库遵循 [LICENSE](./LICENSE)。
