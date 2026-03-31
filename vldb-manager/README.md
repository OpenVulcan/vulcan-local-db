# vldb-manager

`vldb-manager` 是一个基于 Rust 2024 和 `ratatui` 的终端管理界面，用来统一控制同仓库内的：

- `vldb-lancedb`
- `vldb-duckdb`

它的目标是把原本分散在 shell / PowerShell 脚本里的本地控制动作收口到一个跨平台 TUI 里。

## 当前能力

- 自动发现工作区根目录
- 读取 `vldb-lancedb` 与 `vldb-duckdb` 的配置约定
- 一键复制示例配置为工作区配置
- 选择 `debug` / `release` 构建
- 直接在界面里构建、启动、停止、重启服务
- 实时查看构建输出和服务 stdout / stderr
- 探测端口健康状态，区分“当前会话启动”和“外部已运行”

## 运行

```bash
cd ./vldb-manager
cargo run
```

## 快捷键

- `b`: 构建当前服务
- `g`: 生成当前服务配置文件
- `s`: 启动当前服务
- `x`: 停止当前服务
- `r`: 重启当前服务
- `p`: 切换 `debug` / `release`
- `a`: 启动全部服务
- `z`: 停止全部服务
- `↑ ↓` / `j k`: 切换服务
- `← →` / `h l`: 切换视图
- `q`: 退出管理器

## 注意

- 管理器只能停止“由当前管理器会话启动”的进程。
- 如果端口已经被外部进程占用，界面会显示 `EXTERNAL`，但不会强制接管或终止它。
