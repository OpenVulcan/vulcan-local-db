# Docker 安装与使用

## 说明

本仓库现在为两个服务都提供了 Docker 安装文件：

- `vldb-lancedb/Dockerfile`
- `vldb-duckdb/Dockerfile`
- 根目录 `docker-compose.example.yml`

这两个镜像都会在容器内使用项目自带的 Docker 配置文件，并把数据目录放在 `/app/data`。
程序本身仍然会把相对 `db_path` 解释为“相对于配置文件所在目录”，但 Docker 配置为了避免歧义，直接使用了绝对 `/app/data/...` 路径。

如果你只是想直接安装 Docker Hub 上已经发布好的镜像，可以查看 [`docker-install.zh-CN.md`](./docker-install.zh-CN.md)。

## 前置要求

- 已安装 Docker
- 如果要一键同时启动两个服务，建议安装 Docker Compose v2

## 1. 构建单个镜像

### 构建 vldb-lancedb

```bash
docker build -t vulcan/vldb-lancedb:local ./vldb-lancedb
```

### 构建 vldb-duckdb

```bash
docker build -t vulcan/vldb-duckdb:local ./vldb-duckdb
```

## 2. 运行单个容器

### 运行 vldb-lancedb

```bash
docker run -d \
  --name vldb-lancedb \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  -v ./vldb-lancedb/docker/vldb-lancedb.json:/app/config/vldb-lancedb.json:ro \
  vulcan/vldb-lancedb:local
```

### 运行 vldb-duckdb

```bash
docker run -d \
  --name vldb-duckdb \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  -v ./vldb-duckdb/docker/vldb-duckdb.json:/app/config/vldb-duckdb.json:ro \
  vulcan/vldb-duckdb:local
```

## 3. 使用 Compose 一键启动

```bash
docker compose -f ./docker-compose.example.yml up -d --build
```

停止：

```bash
docker compose -f ./docker-compose.example.yml down
```

## 4. 配置方式

容器默认使用：

- `vldb-lancedb/docker/vldb-lancedb.json`
- `vldb-duckdb/docker/vldb-duckdb.json`

其中：

- `vldb-lancedb` 的容器配置把 `host` 改成了 `0.0.0.0`
- `vldb-duckdb` 的容器配置同样监听 `0.0.0.0`
- `vldb-lancedb/docker/vldb-lancedb.json` 使用 `/app/data`
- `vldb-duckdb/docker/vldb-duckdb.json` 使用 `/app/data/duckdb.db`
- 两个服务都把数据目录写到 `/app/data`

如果你要自定义配置，可以直接改挂载进去的 JSON 文件。

### 替换配置文件

如果你想使用自己的配置文件，有两种常见方式。

方式一：直接覆盖默认挂载路径。

```bash
docker run -d \
  --name vldb-duckdb \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  -v ./configs/my-duckdb.json:/app/config/vldb-duckdb.json:ro \
  vulcan/vldb-duckdb:local
```

方式二：把配置文件挂到别的位置，再用 `--config` 指定。

```bash
docker run -d \
  --name vldb-duckdb \
  -p 50060:50060 \
  -v vldb-duckdb-data:/app/data \
  -v ./configs/my-duckdb.json:/app/config/custom.json:ro \
  vulcan/vldb-duckdb:local \
  --config /app/config/custom.json
```

`vldb-lancedb` 也是同样方式，只需把文件名改成 `vldb-lancedb.json` 或在启动参数里改成对应路径。

## 5. 端口与数据目录

- `vldb-lancedb`：容器端口 `50051`
- `vldb-duckdb`：容器端口 `50052`
- `vldb-lancedb` 数据目录：`/app/data`
- `vldb-duckdb` 数据目录：`/app/data`

建议使用这些持久化卷：

- `vldb-lancedb-data`
- `vldb-duckdb-data`

### 修改端口

如果你只想改宿主机端口，不改容器内监听端口，只需要改 `-p` 左边：

```bash
-p 60052:50052
```

这表示容器里仍然监听 `50052`，宿主机改为访问 `60052`。

如果你想连容器内部监听端口也改掉，需要同时改：

- 配置文件里的 `port`
- `docker run` 或 compose 里的端口映射

例如把 `vldb-duckdb` 改成 `50060`：

```json
{
  "host": "0.0.0.0",
  "port": 50060,
  "db_path": "/app/data/duckdb.db",
  "memory_limit": "2GB",
  "threads": 4
}
```

```bash
-p 50060:50060
```

## 6. 多实例部署

当前镜像是“一个容器一个进程一个端口”的模式。  
如果你需要同一个镜像启动多个不同端口的服务，推荐方式是起多个容器实例，每个实例使用：

- 独立的容器名
- 独立的配置文件
- 独立的宿主机端口
- 独立的数据卷或独立的 `db_path`

下面是 `vldb-duckdb` 的 compose 示例：

```yaml
services:
  vldb-duckdb-50052:
    image: vulcan/vldb-duckdb:local
    container_name: vldb-duckdb-50052
    ports:
      - "50052:50052"
    volumes:
      - vldb-duckdb-data-50052:/app/data
      - ./configs/vldb-duckdb-50052.json:/app/config/vldb-duckdb.json:ro

  vldb-duckdb-50062:
    image: vulcan/vldb-duckdb:local
    container_name: vldb-duckdb-50062
    ports:
      - "50062:50062"
    volumes:
      - vldb-duckdb-data-50062:/app/data
      - ./configs/vldb-duckdb-50062.json:/app/config/vldb-duckdb.json:ro

volumes:
  vldb-duckdb-data-50052:
  vldb-duckdb-data-50062:
```

`vldb-lancedb` 也可以照同样方式部署。  
注意不要让多个实例共用同一个 LanceDB 数据目录或同一个 DuckDB 数据文件。

## 7. 当前 compose 的分组方式

仓库里的 [docker-compose.example.yml](../docker-compose.example.yml) 已经按服务分组好了：

- `vldb-lancedb` 作为一组独立服务
- `vldb-duckdb` 作为一组独立服务
- 每组服务各自挂自己的配置文件
- 每组服务各自挂自己的命名卷

当前默认分组对应关系是：

- `vldb-lancedb` -> `vldb-lancedb-data`
- `vldb-duckdb` -> `vldb-duckdb-data`

如果后续扩成多实例，建议继续沿用这个分组思路：

- 按服务类型分组
- 按端口或实例名拆分配置文件
- 按实例拆分数据卷

## 8. 注意事项

- `vldb-lancedb` 的原始示例配置默认监听 `127.0.0.1`，容器里不能直接用，所以镜像改用了专门的 Docker 配置
- 如果你只想跑其中一个服务，可以直接用对应子项目里的 `Dockerfile`
- 在 Windows 的 Docker Desktop 环境下，`vldb-lancedb` 已实际验证使用 Docker 命名卷更稳定；宿主机 bind mount 可能触发 LanceDB 元数据 I/O 异常
- 这些 Docker 文件已在 2026-03-26 实际完成 `docker build`、`docker compose up` 和 Go 客户端联调验证
