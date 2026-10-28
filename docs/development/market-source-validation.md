# A 股 / 港股来源实测

2026-10-26，在本机使用随包方案的 CPython 3.13.17 + AKShare 1.19.3 及锁定的 29 个依赖归档实测；请求为公开样例证券，不写入个人账本。

| 来源 | 样例 | 实测结果 | 约耗时 | 时间限制 |
| --- | --- | --- | --- | --- |
| stock_zh_a_spot / 新浪 | CN:600036 股票 | 返回 CNY 42.22 | 22.71 秒 | 原文 14:43:56，仅时分秒，没有完整日期 |
| stock_hk_spot / 新浪 | HK:00700 股票 | 返回 HKD 386.8 | 20.60 秒 | 原文 2026/10/26 14:28:01，接收约 14:43，约 15 分钟延时 |
| fund_etf_spot_em / 东方财富 | CN:510300 ETF | 当前网络 ProxyError | 9.29 秒 | 未取到报价，未当作零 |
| fund_etf_category_sina / 新浪 | CN:510300 ETF | 返回 CNY 4.172，选为本次 ETF 来源 | 2.12 秒 | 不提供报价时间，保持未知 |

公开来源存在分页、连接失败和限频。整批抓取至少间隔 60 秒，后台常规时段持续刷新，失败退避至最多 15 分钟；界面约 5 秒读取缓存估值。常规时段不包含完整节假日日历，不以系统收到数据的日期补造缺失报价日期。不把更新旧报价称作新实时成交。

官方依据：[AKShare 股票接口](https://akshare.akfamily.xyz/data/stock/stock.html)、[AKShare 基金接口](https://akshare.akfamily.xyz/data/fund/fund_public.html)、[CPython 3.13.17](https://www.python.org/downloads/release/python-31317/)。新浪港股接口文档注明约 15 分钟延时，A 股接口文档提示重复运行可能暂时封 IP。

独立运行时全部归档 URL / SHA256 见 market/runtime-lock.json，依赖从 PyPI 原始发布文件核对哈希。CPython 由官方 Windows 嵌入包分发；jsonpath 源归档只复制模块、元数据及许可，不执行 setup.py。

scripts/verify-market-runtime.py 已在 PATH 为空、PYTHONHOME/PYTHONPATH 指向不存在目录、工作目录为临时空目录时完成离线健康检查。它验证精确的包内解释器和版本，不冒称已在本机卸载 Python；完整干净机器发布验收仍由第 16 项完成。美股接入及其 yfinance 依赖由下一项交付。
