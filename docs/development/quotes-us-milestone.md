# 第 9 项：美股行情（Issue #10）

基准 f28fe56fc14495331a2fd110c500d0ae840f0872（第 8 项已安装版本之后）。沿用已确认的公开 Ledger 完整应用用例及少量完整 App 流程，以 Tauri IPC 模拟外部边界。新增美股公开检查页面流程红灯（入口缺失）后实现到绿灯；核心回归检查估值、历史缓存、断线不改数量、冬夏令时及分批公平性。

通过 yfinance 1.7.0 调用 Yahoo Finance，取得 USD 股票 / ETF 的常规市场最近成交报价与近一个月日线 Close。请求明确 auto_adjust=false / back_adjust=false / repair=false / actions=false / prepost=false，不将历史价当成持仓收益率，不自动生成分红或拆股账户事件。Yahoo 日线仍可能按拆股调整，当天日线可能未收盘；界面明确标注此口径。历史曲线及更长历史范围由后续收益计算任务确定。

来源采用 regularMarketPrice 与 regularMarketTime；后者在锁定 yfinance 中可能已经转换为带时区 Timestamp，兼容该公开属性值及 epoch。严格核对返回 symbol、USD、EQUITY / ETF、America/New_York；BRK.B 仅在请求适配层映射到 BRK-B，保留原账本代码。价格按六位小数保留，来自浮点的源日线可能有微小尾差，报价与日线是两个来源字段，不强行相等。

沿用独立行情库，报价、来源、完整时点、日线一起缓存；零值、非法日线日期/价格、缺失、断线、旧时间不覆盖最后有效缓存。主账本 schema7、加密备份 v6 不变。无报价时估值显示数据不足。Yahoo cookie / 时区元数据在单独 market-provider-cache 目录，由提供方库管理，不混入个人账本或备份。

纽约常规时段 09:30–16:00（周一到周五）使用 chrono-tz 的 IANA 规则转换夏令时，仍不冒充节假日日历。后台持续运行，界面约 5 秒读取缓存；来源最短 60 秒，失败退避至 15 分钟。每轮最多 10 项美股、并发最多 4，优先最久未尝试的证券，避免后续持仓饥饿。进程 85 / 90 秒硬上限沿用。常规时段外可手动刷新，结果仍可能是旧收盘报价；不承诺实时或盘前盘后报价。

## 来源实测

2026-10-26，内置 CPython 3.13.17 / yfinance 1.7.0 / 36 个锁定归档实测，仅请求公开证券，无个人账本写入。约 3.8 秒取得：

| 证券 | 常规报价 USD | 来源时间（纽约） | 日线条数 | 最新日线日期 / Close |
| --- | --- | --- | --- | --- |
| AAPL | 347.725 | 2026-10-23T16:00:01-04:00 | 22 | 2026-10-23 / 347.730011 |
| SPY（ETF） | 759.97 | 2026-10-23T16:00:00-04:00 | 22 | 2026-10-23 / 759.969971 |
| BRK.B → BRK-B | 542.19 | 2026-10-23T16:03:16-04:00 | 22 | 2026-10-23 / 542.190002 |

第一轮实测发现 Windows 临时目录清理时库仍持有 SQLite 句柄，改为库自身管理的独立可丢弃缓存；第二轮核对 yfinance 的 regularMarketTime 实际为带时区 Timestamp，兼容后完整通过。没有使用虚构或旧样例冒充联网验收。

官方依据：[yfinance 源码 / history 参数](https://github.com/ranaroussi/yfinance/blob/main/yfinance/scrapers/history.py)、[时间元数据格式转换](https://github.com/ranaroussi/yfinance/blob/main/yfinance/utils.py)、[PyPI 固定发布](https://pypi.org/project/yfinance/1.7.0/)、[chrono-tz IANA 时区](https://docs.rs/chrono-tz/0.10.4/chrono_tz/)。全部下载哈希核对 PyPI 原始归档，许可证保留；安装包不执行 pip、不依赖系统 Python。

## 交付验证

第一轮 Linux 回归 [40291209960](https://github.com/tallmye315/Graham/actions/runs/40291209960) 成功（Windows 按 ledger-only 跳过）。22 项界面测试、类型检查通过。双轴审查均指出同一 P2：Python 静默丢弃零值日线；已通过内置进程 stdin/stdout 公共协议与模拟的外部 yfinance Ticker 复现红灯，改为拒绝整项后转绿，核心非法日线沿用缓存回归也通过。离线验证不访问 Yahoo，也不修改个人账本。最终增量复核 Standards 原 1 项已关闭、剩余 0；Spec 原 1 项已关闭、剩余 0。完整无 Python 干净机器发布验收仍在第 16 项。

## 0.1.13 交付证据

源码 `9763b01`，最终构建 [40315494818](https://github.com/tallmye315/Graham/actions/runs/40315494818) 整体 success。Linux / Windows 各 47 项账本回归、22 项界面测试、类型检查、公司 Logo 资源检查、随包运行时构建、离线隔离及非法日线协议回归、NSIS 打包、真实安装后的运行时 / 快捷方式 / 原生图标检查均通过。

安装包 `artifacts/windows-0.1.13/target/release/bundle/nsis/Graham_0.1.13_x64-setup.exe`，SHA256 `1b0fd4a8f1fe889e73aecb07e2dd7605116f9c3327c8603b601fc61c874a9a6d`。已在用户本机实际安装并打开，注册版本 0.1.13；安装 exe 经 Tauri UNK → NSS bundle 标记规范化后与下载 CI 二进制一致，worker / runtime-lock 在 Windows 换行规范化后与源码一致。

安装后的资源绝对路径调用隔离解释器检查成功：CPython 3.13.17、AKShare 1.19.3、yfinance 1.7.0、36 个精确归档，在空 PATH、无效 PYTHONHOME/PYTHONPATH 及无关工作目录下成功导入并完成离线协议回归。没有依靠开发机系统 Python 完成内置进程启动；完整无 Python 干净机器验收仍属于第 16 项。

真实原生 UI “行情 → 检查 A 股 / 港股 / 美股连接”取得 AAPL 347.725 USD（来源 2026-10-23T16:00:01-04:00）、SPY 759.97 USD（来源 2026-10-23T16:00:00-04:00），接收 2026-10-26T11:49:46Z。各 22 条日线从 2026-09-24 到 2026-10-23 已在历史日线核对表展示，最新 Close 分别为 347.730011 / 759.969971 USD。检查时纽约尚未进入常规交易时段，界面准确保留上周五来源时间。A 股 / 港股连接也正常，原有缓存读取兼容。

升级前后及实际行情检查后，个人主账本文件 SHA256 与所有业务表全部原记录 / 修订历史的逻辑指纹一致，完整性正常，schema7 / 备份 v6 不变。私人指纹仅存于 ignored artifacts，未上传到 GitHub。桌面 / 原生任务栏图标分别检测到 29 / 35 个酒红像素。

主线进度 9/16，下一项 Issue #11 多币种估值。软件留在显示美股报价与历史核对的行情页。本项只提供近一个月的历史价格核对，不宣称收益率曲线或券商自动交易同步已经实现。
