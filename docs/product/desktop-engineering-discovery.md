# Graham 桌面工程决策访谈

日期：2026-10-24。

状态：本轮工程决策已确认。正式安装包自带固定 Python 运行时及依赖，开发环境与发布依赖分别验证。本文记录实现前的方案与验证要求，当前能力以 README 为准。

## 沿用的已确认约束

- 个人使用、中文 Windows 桌面、本地记录和备份恢复，无云端同步。
- UI 采用 A 布局、酒红与白色；前端已调研 React、shadcn/ui、ECharts，尚未迁移。
- 华龙证券、兴业证券优先文件导入，允许手动修正；确认前预览、校验、去重。
- 行情与账户交易分开更新。0 元行情预算、约 5 秒估值刷新目标，允许明确标记的延时或有日期价格。
- 历史不完整时使用经核对的期初快照，不编造更早收益。
- 原型三项审查缺陷已修复；正式记录、收益引擎和安装产物仍待实现与验收。

## 本轮已查证的事实

- Windows 开发环境需准备 Node.js、Rust MSVC、C++ Build Tools、Windows SDK 与 WebView2；构建可用性通过实际编译和安装验证。
- 开发用 Python 与发布包内置运行时分别管理。AKShare/yfinance 及运行时组合需在独立环境中验证，不能依赖某台开发机已有的安装状态。
- Tauri 2 可使用 React 前端和 Rust 原生能力；Windows 使用 WebView2，安装器支持处理缺失运行时，也可嵌入离线运行时安装器。离线安装与小安装包需要取舍。[架构](https://v2.tauri.app/concept/architecture/)、[开发依赖](https://v2.tauri.app/start/prerequisites/)、[安装](https://v2.tauri.app/distribute/windows-installer/)
- Electron 随应用分发 Chromium 和 Node，用户无需另装 Node/WebView2；内置运行时也需随应用维护。没有对两个方案作安装体积、内存或速度的实测承诺。[框架](https://www.electronjs.org/docs/latest/why-electron)、[进程模型](https://www.electronjs.org/docs/latest/tutorial/process-model)
- SQLite 适用于桌面应用本地文件。事务可覆盖一次确认导入；金额精度应明确，避免浮点累计。备份应使用一致快照，运行中不能只复制主数据库而忽略 WAL。[适用场景](https://www.sqlite.org/whentouse.html)、[精度](https://www.sqlite.org/floatingpoint.html)、[备份](https://www.sqlite.org/backup.html)、[WAL](https://www.sqlite.org/wal.html)
- Tauri 自动更新需要校验更新签名；它与 Windows 程序代码签名不同。官方有 GitHub Releases 发布流程；本项目尚未配置更新端点、发布密钥或证书。[更新](https://v2.tauri.app/plugin/updater/)、[发布](https://v2.tauri.app/distribute/pipelines/github/)
- 券商专用格式需用经授权且已脱敏的 CSV/XLS/XLSX 样例验证。华龙和兴业均有多个终端，公开资料不能保证不同导出文件的列名、历史跨度和同步权限一致。[华龙下载](https://download.hlzq.com/main/download/index.shtml)、[兴业客户端](https://www.xyzq.com.cn/xysec/info/article/ed957a6ff3f94175b8256bd944d5609a)、[兴业 SMT](https://smt.xyzq.com.cn/)

## 设计树与本轮问题

1. 桌面框架：已确认 Tauri 2 + React/TypeScript，基础 UI 使用 shadcn/ui，图表采用 ECharts；原生端负责本地文件和数据库能力。
2. 本地存储：已确认 SQLite 保存在 Windows 用户数据目录，修改自动保存、事务入账、应用内备份与恢复，恢复前先备份当前账本，金额采用明确精度。
3. 数据保护：已确认首版不设置软件登录密码；本机账本依靠 Windows 用户权限保护，数据库本身不加密；提供加密导出备份。Q11 已确认由用户输入并保管备份密码，软件不保存该密码，遗失后无法解密该备份。
4. 安装更新：已确认当前用户安装的 .exe、手动下载安装新版、保留账本与缓存；升级前备份，迁移失败保留恢复路径，内置自动更新延后。
5. 券商接入：已确认按实际样本适配文件，保留原始行、导入批次和修正记录，逐条去重与校验；优先核对交割单、资金流水、持仓快照，未知事件进入待处理。
6. 数据覆盖：使用 AKShare / yfinance 获取市场数据。行情来源不能推导券商账户信息或自动同步权限；实际交易以导入或手动登记的记录为准。

本轮已覆盖框架、存储、数据保护、安装更新、账户记录与市场数据来源。实际接口、券商样本和安装产物留待工程验证，技术实现细节在相应任务中落实。已确认的重要取舍记录于 docs/adr/。

## AKShare / yfinance 核对与第二轮

- 两者提供公开市场数据，不提供个人成交、入出金及实收税费。账户记录通过文件导入与手动录入维护。[AKShare](https://github.com/akfamily/akshare)、[yfinance](https://github.com/ranaroussi/yfinance)
- AKShare 有 A 股、港股、美股等接口，具体上游、字段及覆盖各异；yfinance 使用 Yahoo 市场数据，能提供价格历史、分红拆股等。采用这些来源不代表每个市场的时效、历史跨度或接口可达性都已实测通过。[AKShare 股票文档](https://akshare.akfamily.xyz/data/stock/stock.html)
- 两者均为 Python 库；Tauri 可以通过打包的外部数据进程接入。若要求用户无需另装 Python，需把运行时和依赖一并打包，包大小和启动表现待实测。[Tauri 外部程序](https://v2.tauri.app/develop/sidecar/)
- 数据适配必须明确复权与公司行动口径，避免将已含分红的复权收益再加实收分红；来源错误、限流和服务不可用应保留最后有效数据与其时间，不承诺免费三市场持续 5 秒产生有效报价。[yfinance 下载参数](https://ranaroussi.github.io/yfinance/reference/api/yfinance.download.html)
- yfinance 官方提醒 Yahoo 数据面向个人使用；当前产品仍以个人使用为范围，该表述不能泛化为 AKShare 的所有上游。[yfinance 官方说明](https://github.com/ranaroussi/yfinance)

第二轮确定了市场来源、内置运行时、导入范围、离线安装与备份密码的处理方式：

- Q7 已确认：AKShare 优先负责 A 股、港股，yfinance 优先负责美股；具体接口先实测。后台批量取数、缓存和退避重试，页面保留来源与报价时间；约 5 秒估值刷新继续作为目标，数据时效依据实际来源显示。
- Q8 已确认：本机 Python 用于开发，正式软件内置固定运行时和依赖。正式安装产物不依赖系统 Python；接受安装包增大，发布版本及依赖组合由兼容性验证确定并固定。
- Q9 已确认：先贯通手动录入和统一 CSV 导入；券商专用格式仍以脱敏样本核对，华龙和兴业的适配目标保留。样本未知不阻塞本地账本的先行实现。
- Q10 已确认：优先干净机器离线安装，带齐必要运行时，包括 WebView2 离线安装程序和 Q8 确认的 Python 数据进程及依赖；接受安装包增大。
- Q11 已确认：加密导出备份由用户设置和保管密码，软件不保存，遗失无法解密该备份。

本轮访谈收敛完成。实际网络可用性、券商样本适配和 Windows 安装验证属于后续工程验收，不能写成已经通过。

## 正式工程的验证起点

- 首条可演示流程：安装启动、录入一笔记录、关闭重开仍可查询该记录。
- 干净机器未安装 Python 时也能离线安装和启动；已有账本和内置 Logo 可读取，联网数据不可用时显示相应状态。
- Python 数据进程异常不损坏账本，按来源保留缓存与数据时点；运行时和依赖组合需兼容性检查后固定。
- 统一 CSV 预览、校验、确认入账与重复导入核对；华龙、兴业专用适配通过各自真实脱敏样本另行验收。
- 在已有 PRD 的完整应用用例验收范围内验证成本、收益、分红、汇率、备份恢复与升级迁移；模拟原型不是这些能力的验收凭证。
