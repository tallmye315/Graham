# Windows 任务栏图标修复

0.1.1 的任务栏图标曾出现白纸图标，已有快捷方式验收未覆盖原生窗口大小图标的差异。

## 复现与边界

通过真实进程 PID 枚举标题为 Graham 的应用窗口，再读取 WM_GETICON / 窗口类图标，渲染图像并核对酒红色像素。不能使用 Get-Process.MainWindowHandle：Tao 的隐藏事件窗口可能先被返回，导致检测错窗口。

复现版本 0.1.1：

```powershell
$grahamProcessId = (Get-Process -Name graham).Id
python scripts/verify-windows-icons.py --process-id $grahamProcessId
# exit 1: Window has no taskbar large icon
python scripts/verify-windows-icons.py --process-id $grahamProcessId --small
# exit 0: burgundyPixels=35
```

桌面快捷方式图标提取通过（29 个酒红像素），不能替代任务栏验收。测试属于既有原生安装应用外部行为边界，不访问金融记录，不修改运行中的窗口。

## 原因与修复方向

锁定的 Tao 0.37.2 源码中，set_window_icon 只设置 IconType::Small，set_taskbar_icon 独立设置 IconType::Big；现 Tauri 默认窗口路径仅具备小图标。启动时为实际主窗口显式设置独立拥有的大图标，复制已有品牌图标句柄并在应用退出后释放，避免借用句柄失效。不修改应用身份、品牌设计或账本。

来源：[Tao 源码](https://github.com/tauri-apps/tao/blob/dev/src/platform_impl/windows/window.rs)、[WM_GETICON](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-geticon)、[CopyIcon 生命周期](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-copyicon)、[WM_SETICON](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-seticon)。实际分析还下载并核对了 Cargo.lock 指定的 Tao 0.37.2 发布包 SHA256。

## 验收要求

- 冷启动和桌面快捷方式重开后，真实主窗口均可提取正确的大图标、小图标。
- 实际 Windows 任务栏显示正式蛇环短 Logo；不能仅以桌面快捷方式和文件存在作为完成依据。
- 图标验证不清除系统缓存、不重启资源管理器、不写入账本。
- Windows CI 启动构建的真实 EXE，执行上述检查，以防后续打包回归。

## 0.1.2 阶段记录（后续复现，非最终验收）

已交付 0.1.2，应用代码 `db1a3c9`。[最终流水线 40164331571](https://github.com/tallmye315/Graham/actions/runs/40164331571) 的 11 项账本测试、7 项界面测试、类型检查、Windows 构建及真实 EXE 图标检查全部通过。任务栏大图标和标题栏小图标均提取出 35 个酒红像素。

安装验收确认注册版本为 0.1.2，已安装 EXE 与该流水线原始 EXE 在规范化 Tauri NSIS marker 后逐字节一致。首次启动检测大图标、小图标均通过，桌面快捷方式图标提取通过。

通过桌面快捷方式重新启动验证进程后，任务栏大图标检测仍通过（35 个酒红像素）。图标验证仅检查窗口和图像，不写入账户或资金记录。

- 安装包：`artifacts/windows-0.1.2/target/release/bundle/nsis/Graham_0.1.2_x64-setup.exe`。
- 安装包 SHA256：`c7f5c29db21f1ca29fa67837f0f2c6902e3262b5a4a41ecc998414c4928b1087`。
- 已安装 EXE SHA256：`bc2e4ae17f87c1a2791f9b774eb94d75b296f22e4723a0b5268c652ce7060330`。

双轴审查以 `main` 为固定点，命令 `git diff main...HEAD`。审查发现 CI 最初仅等待窗口出现，没有等待图标初始化；已改为同一截止时间内重复定位窗口及验证实际图标，成功才提前结束，超时保留最后诊断。复核后 Standards 硬性违反 0、判断项 0；Spec 剩余发现 0。

此前只验证快捷方式文件和标题栏图标的验收不足；现在把任务栏大图标检查固化进 Windows 构建，避免将“文件存在”当成任务栏显示通过。

后续回归再次复现桌面白纸图标及任务栏异常，因此上述阶段结论不能代表最终修复，Issue #19 重新进入修复流程。

## 2026-10-25 再次复现与路径修复

错误快捷方式的 TargetPath 和 WorkingDirectory 指向物理安装目录，但 IconLocation 使用 `%LOCALAPPDATA%/Graham/graham-icon.ico` 的重定向别名，开始菜单链接还存在旧路径和空图标引用。进程可以读取路径别名并提取像素，不代表 Windows Shell 能正确展示图标。

在图标提取之前增加快捷方式元数据检查，错误样例返回：`targetUsesPhysicalPath=true`、`iconUsesPhysicalPath=false`、`iconSharesTargetDirectory=false`。修正桌面和开始菜单的引用并发送单项 Shell 更新通知后，图标检测通过。

0.1.3 将该修复固化为共同的快捷方式修复入口：

- 使用 `dunce::canonicalize` 解析 EXE 和 ICO 的实际物理路径，保持目标、图标和工作目录一致。
- 软件内创建入口与安装器 POSTINSTALL 都调用同一实现；安装器调用 `--repair-shortcuts` 时不创建窗口、不打开账本，失败以退出状态返回。
- 更新本软件的桌面、开始菜单及已存在的本软件任务栏快捷方式，并对变更项发出 `SHChangeNotify`。任务栏链接按完整实际目标路径确认归属，支持重命名的链接；不同安装、无法确认归属的链接保持原样，不修改系统缓存设置。
- Windows CI 改为运行真实 NSIS 安装器，检查已安装版本；再从目录联接别名调用修复入口，检查重命名的本安装任务栏链接由红转绿、其他同名程序链接的文件哈希不变、桌面和开始菜单的物理路径及图像，最后启动安装版检查真实窗口大/小图标。

来源：[兼容 Windows Shell 的规范路径](https://docs.rs/dunce/latest/dunce/fn.canonicalize.html)、[Shell 变更通知](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shchangenotify)。

## 0.1.3 固化与安装验收

0.1.3 的应用代码为 `15518ef`。[流水线 40166837450](https://github.com/tallmye315/Graham/actions/runs/40166837450) 全部通过：11 项账本测试、7 项界面测试、类型检查、Windows 打包，以及上述安装与快捷方式、窗口图标回归检查。目录联接错误样例先明确失败，修复后通过。

安装验收确认注册版本为 0.1.3；已安装 EXE 与该流水线原始 EXE 在内存中规范化 Tauri NSIS marker 后逐字节一致。通过桌面快捷方式启动后，桌面、开始菜单链接元数据与图像均通过，原生窗口大/小图标均提取出 35 个酒红像素，任务栏显示 Graham 短标。

通过软件内“创建桌面快捷方式”入口创建链接后，桌面和开始菜单的物理路径一致，图像检测通过（29 个酒红像素）。手工修复、安装升级、重新创建入口和原生窗口检查分别验收，不以某一环节的结果替代完整回归。

双轴审查固定基准为 `2767e7e3173b52f9c77d3fe50c41c6bc01b2b391`（main），最终代码 `15518ef`。审查发现的安装进程退出状态读取及任务栏链接归属问题均已修复：安装测试保留实际进程句柄并检查退出状态；链接按完整物理目标路径识别，并补充重命名和不同安装的回归场景。复核 Standards 硬性违反 0、判断项 0；Spec 剩余发现 0。

- 安装包：`artifacts/windows-0.1.3/target/release/bundle/nsis/Graham_0.1.3_x64-setup.exe`。
- 安装包 SHA256：`fae0bd4efd6b1112a5bf172e47dcbe541d6dcf61c902a0a7b79867642451c44c`。
- 已安装 EXE SHA256：`c77867e86e6159de1f9d749ae9548390cf31d0f5f581172c42a21ace0f1731d8`。
- 公开验收记录仅包含技术结论，诊断原件和非合成账户截图不纳入源码仓库。

状态：修复已固化并安装，回归检查通过。完整离线发布和干净 Windows 验收仍属于 Issue #17，本次图标修复不代表全部产品能力已完成。
