# VLDB 安装脚本审查与修复结果

## 已处理的文件
- install.sh
- vldb
- install.ps1
- vldb.ps1

## 关键问题
1. **Bash 脚本原始文件含 CRLF/混合换行，Linux/macOS 下会直接语法报错**。
2. **已有安装时，install 脚本会无条件把控制权交给旧版 manager**，离线或 GitHub 不可达时，新安装包里的修复无法落地。
3. **Bash manager 的数据路径校验可被 `..` 路径绕过**，还允许带双引号的路径写进手工 JSON，存在配置损坏风险。
4. **Bash manager 的 `validate_data_path` 把错误文本写到 stderr，而调用方用命令替换取 stdout**，导致部分非法路径会被当作“校验通过”。
5. **Shell profile 只写单个 profile 文件**，bash 常见场景下新终端不一定立刻拿到 PATH。
6. **无 TTY 场景会冒 `/dev/tty` 噪声**，影响 CI/自动化执行体验。
7. **Windows manager 下载指定 tag 二进制时，实际仍拿 latest release 元数据做校验**，release tag 与 asset 元数据不一致时会失败。
8. **Windows installer 也存在“旧 manager 抢先接管”的同类问题**。

## 已实施修复
- 全部脚本统一为 LF 换行。
- 本地热修版本号统一提升到 `0.1.24`。
- Bash 侧新增绝对路径规范化与 JSON 安全字符校验。
- Bash 侧修复 `validate_data_path` 的 stdout/stderr 语义错误。
- Bash installer 新增“若安装包内置 manager 更高版本，则先覆盖本地旧 manager 再移交控制权”。
- Windows installer 同步加入 bundled manager 优先刷新逻辑。
- Windows manager 新增 `Get-ReleaseByTag`，修复按指定 tag 下载 release 的元数据错误。
- Shell profile 更新改为覆盖 `.bash_profile/.bashrc/.profile` 或 zsh 的 `.zprofile/.zshrc`。
- Bash 脚本新增 `VULCANLOCALDB_TEST_MODE` 守卫，便于自动化测试与后续 CI。
- 无 TTY 场景下的 `/dev/tty` 打开逻辑已静默处理。

## 已执行验证
### Bash 语法验证
- `bash -n /mnt/data/fixed/install.sh`
- `bash -n /mnt/data/fixed/vldb`

### Bash 行为验证
- 路径规范化（`/tmp/a/../b` -> `/tmp/b`）
- 带双引号/换行路径拒绝
- 安装目录重叠判断可识别 `..` 绕过
- manager 数据路径冲突检测
- `write_service_config` 可拒绝 unsafe JSON path
- installer 可用 bundled manager 覆盖旧版本地 manager

## 输出文件
- 修复后的脚本位于当前目录
- 每个原文件对应一个 `.diff` 供审阅
