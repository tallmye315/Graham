# Windows 发布验收

公开版本 v0.1.0 以 [Pre-release](https://github.com/tallmye315/Graham/releases/tag/v0.1.0) 发布，提供 Windows x64 当前用户安装包及 SHA-256 校验文件。安装包未进行代码签名。当前功能与限制见 [README](../../README.md)。

## 已通过的自动化检查

[v0.1.0 构建运行](https://github.com/tallmye315/Graham/actions/runs/40695781093) 对应提交 `abe8fc8ceba83438461a2250bddd7707d1213023`：

- Linux / Windows 账本回归、35 项界面测试、类型检查及生产构建。
- NSIS 安装后核对版本、内置 Python 运行时、桌面及开始菜单快捷方式。
- 从无关目录以空 PATH、无效 PYTHONHOME / PYTHONPATH 验证运行时隔离及启动。
- 验证原生窗口与任务栏图标，以及快捷方式修复不会修改其他安装目录的同名程序。

上述检查不代表已完成完全断网、无 Python / WebView2 的干净 Windows 安装验收。

## 独立干净 Windows 验收：待完成

1. 在独立 Windows x64 测试机或虚拟机保存初始快照，确认未安装 Python 和 WebView2，使用空白测试账本。
2. 从 Release 复制安装包并核对 SHA-256，断网安装，确认随包 WebView2 离线安装分支成功。
3. 从桌面及开始菜单启动，核对版本与图标；无网络时明确显示行情失败或缺项。
4. 仅创建合成账户和记录，保存、关闭、重开确认；再次安装后核对记录、修改历史和缓存保留。
5. 在独立环境验证加密备份及恢复，保存结果。环境或步骤缺失时继续标为待验收。

升级前导出加密备份并关闭软件。启动或恢复失败时保留数据目录，按 [README 数据与备份](../../README.md#数据与备份) 核对；不得使用真实账本执行破坏性测试。

