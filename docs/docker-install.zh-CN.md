# Docker 安装说明

## 说明

如果你只是想直接安装并运行已经发布好的镜像，可以直接使用下面两个 Docker Hub 镜像：

- `openvulcan/vldb-lancedb:latest`
- `openvulcan/vldb-duckdb:latest`

这份文档使用的是直接 `docker pull` 和 `docker run` 的方式，不需要本地编译源码。

## 前置要求

- 已安装 Docker
- 可以访问 Docker Hub

## 1. 拉取镜像

```bash
docker pull openvulcan/vldb-lancedb:latest
docker pull openvulcan/vldb-duckdb:latest
```

## 2. 启动两个服务

### 启动 `vldb-lancedb`

```bash
docker run -d \
  --name vldb-lancedb \
  --restart unless-stopped \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  openvulcan/vldb-lancedb:latest
```

### 启动 `vldb-duckdb`

```bash
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

## 3. 查看运行状态

```bash
docker ps
docker logs -f vldb-lancedb
docker logs -f vldb-duckdb
```

## 4. 停止或删除服务

停止：

```bash
docker stop vldb-lancedb vldb-duckdb
```

删除容器：

```bash
docker rm vldb-lancedb vldb-duckdb
```

如果你还想一起删掉持久化数据卷：

```bash
docker volume rm vldb-lancedb-data vldb-duckdb-data
```

## 5. 使用自定义配置文件

如果你希望覆盖默认配置，可以把自己的 JSON 文件挂载进容器。

### `vldb-lancedb` 自定义配置

```bash
docker run -d \
  --name vldb-lancedb \
  --restart unless-stopped \
  -p 50051:50051 \
  -v vldb-lancedb-data:/app/data \
  -v /your/path/vldb-lancedb.json:/app/config/vldb-lancedb.json:ro \
  openvulcan/vldb-lancedb:latest
```

### `vldb-duckdb` 自定义配置

```bash
docker run -d \
  --name vldb-duckdb \
  --restart unless-stopped \
  -p 50052:50052 \
  -v vldb-duckdb-data:/app/data \
  -v /your/path/vldb-duckdb.json:/app/config/vldb-duckdb.json:ro \
  openvulcan/vldb-duckdb:latest
```

## 6. 修改端口

如果你只想修改宿主机端口，只改 `-p` 左边即可。

例如：

```bash
docker run -d \
  --name vldb-duckdb \
  --restart unless-stopped \
  -p 60052:50052 \
  -v vldb-duckdb-data:/app/data \
  openvulcan/vldb-duckdb:latest
```

这样容器内部仍监听 `50052`，但宿主机访问地址会变成 `127.0.0.1:60052`。

如果你连容器内部监听端口也要改，就需要同时修改：

- JSON 配置文件里的 `port`
- `-p 宿主机端口:容器端口`

## 7. 可选的 Compose 示例

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

启动命令：

```bash
docker compose up -d
```

## 注意事项

- 这两个镜像提供的是 gRPC 服务，不是 REST 服务。
- `vldb-lancedb` 的数据默认直接写入 `/app/data`。
- `vldb-duckdb` 的数据默认写入 `/app/data/duckdb.db`。
- 默认推荐使用 Docker 命名卷做持久化。
