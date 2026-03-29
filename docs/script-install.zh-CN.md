# 脚本安装说明

## 说明

VulcanLocalDB 提供了可直接从 GitHub 获取的交互式安装脚本，可以用来完成本地安装。

适合以下场景：

- 不想手动解压发布包
- 希望由脚本自动下载匹配平台的 release
- 希望自动生成 `vldb-lancedb` 和 `vldb-duckdb` 的默认配置
- 希望自动安装 `vldg` 管理命令
- 需要按系统注册自动启动服务

仓库地址：

- [OpenVulcan/vulcan-local-db](https://github.com/OpenVulcan/vulcan-local-db)

## 支持的平台

- Linux：`install.sh`
- macOS：`install.sh`
- Windows PowerShell：`install.ps1`

补充说明：

- Linux 和 macOS 安装脚本支持英文与简体中文。
- Windows PowerShell 版本目前只保留英文，因为 Windows PowerShell 5.x 的 UTF-8 支持较差。

## 快速开始

### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.sh | bash
```

### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.sh | bash
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.ps1 | iex
```

如果你的网络路径、代理或 CDN 对 `main` 分支脚本命中了旧缓存，可以把 URL 里的 `main` 替换为仓库当前的具体提交 SHA。

## 安装脚本会做什么

安装脚本可以完成：

- 显示当前安装脚本版本和最新 release tag
- 选择完整安装或仅安装控制脚本
- 选择安装目录
- 配置默认绑定 IP 与端口
- 自动下载 GitHub Release 中对应平台的二进制包
- 安装 `vldb-lancedb` 和 `vldb-duckdb`
- 生成默认配置文件
- 安装 `vldg` 管理命令
- 按系统选择是否注册为自动启动与自动重启服务

全局配置文件路径：

- Linux 和 macOS：`~/.vulcan/vldg/config.json`
- Windows：`%USERPROFILE%\.vulcan\vldg\config.json`

配置文件会记录：

- 当前语言
- 安装目录
- 已安装的 release tag
- 已安装的控制脚本版本

## 依赖处理

当脚本发现缺少依赖时，会先提示，再由用户确认是否自动安装。

典型依赖包括：

- Linux：`curl`、`tar`、`sha256sum` 或等效工具
- macOS：`curl`、`tar`、`shasum` 或等效工具
- Windows：使用 PowerShell 自带下载与哈希能力
- Windows 服务模式：若缺少 WinSW，会在确认后自动下载安装

## 安装完成后如何使用

控制脚本命令为：

- Linux 和 macOS：`vldg`
- Windows：`vldg.cmd`

示例：

Linux 或 macOS：

```bash
vldg
```

Windows：

```powershell
vldg.cmd
```

`vldg` 可以继续做这些事情：

- 查看已安装实例
- 修改 IP 和端口
- 注册或取消注册服务
- 检查脚本版本和 release 更新
- 单独安装或卸载实例
- 卸载管理脚本或全部安装内容

## 相关文档

- 二进制发布包安装说明：[native-install.zh-CN.md](./native-install.zh-CN.md)
- Docker 快速安装说明：[docker-install.zh-CN.md](./docker-install.zh-CN.md)
- LanceDB 服务说明：[vldb-lancedb.zh-CN.md](./vldb-lancedb.zh-CN.md)
- DuckDB 服务说明：[vldb-duckdb.zh-CN.md](./vldb-duckdb.zh-CN.md)
