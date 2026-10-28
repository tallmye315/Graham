# 行情、导入与收益口径：访谈事实依据

查证日期：2026-10-24。本文记录事实与待确认事项，不代表已选定供应商或计算方案。

用户后续已确认免费行情（预算 0 元）、约 5 秒更新估值的目标，以及同时展示时间加权与资金加权收益。供应商权限和具体计算边界仍待验证；产品决策以 `discovery.md` 为准。

## 券商记录导入

- 兴业证券有 SMT/QMT 等专业产品，华龙证券官方历史公告也列出 QMT；这些资料不能证明用户当前账户拥有程序化接口或自动同步权限。
- 两家券商当前客户端的导出列名、文件格式及可查询历史跨度，尚未取得足以实施适配的公开规范。后续以用户实际客户端导出的脱敏样本核对。
- 券商名称、港股通、直接海外账户属于不同信息，需按实际账户记录确定。

来源：

- [兴业官方渠道公示](https://www.xyzq.com.cn/xysec/info/article/8143a6b2a25d453b9f49a3f525b80b21)
- [兴业 SMT 产品](https://smt.xyzq.com.cn/product.html)
- [华龙官方维护公告](https://download.hlzq.com/main/a/20230721/72045694.shtml)

## 盘中行情

- 富途和长桥的官方接口提供多市场行情能力。行情权限、订阅限制与实际时效需要按服务账号核验，不能从券商文件导入能力推导出三市场实时行情权限。
- 富途明确说明 API 与 App 行情权限不完全相同；部分市场的可用权限还与登录 IP 等条件有关。
- 长桥官方定价和权限页面对推广免费与港股实时权限的表述存在差异，最终需要核验用户账户实际权限。自动刷新、交易所实时、延时行情是不同概念。
- 是否使用额外行情账号、是否接受付费、更新目标以及断线后的展示方式，仍需用户决定。

来源：

- [富途权限说明](https://openapi.futunn.com/futu-api-doc/intro/authority.html)
- [长桥接口介绍](https://open.longbridge.com/zh-CN/docs)
- [长桥定价](https://open.longbridge.com/zh-CN/pricing)
- [长桥行情权限概览](https://open.longbridge.cn/zh-CN/docs/quote/overview)

## 收益口径

- 时间加权收益率用于观察排除外部资金进出直接影响后的投资表现；资金加权收益率反映投入金额与时点。具体展示口径尚待确认。
- 分红收益的确认与现金到账可以发生在不同日期。是否记录待到账分红以及税费净额，需要明确。
- 合并账户内的转账如何与单账户资金进出对应，需要明确计算范围，并将两端关联。
- 历史价格可能已经包含分红或拆股调整。后续实现必须协调价格序列与实际账本，避免重复计入同一经济事件。

来源：

- [GIPS Handbook](https://www.gipsstandards.org/standards/gips-standards-for-firms/gips-standards-handbook-for-firms/)
- [Portfolio Performance：时间加权收益](https://help.portfolio-performance.info/en/concepts/performance/time-weighted/)
- [Portfolio Performance：资金加权收益](https://help.portfolio-performance.info/en/concepts/performance/money-weighted/)
- [Portfolio Performance：分红记录](https://help.portfolio-performance.info/en/reference/transaction/dividend/)
- [Portfolio Performance：历史价格](https://help.portfolio-performance.info/en/concepts/historical-prices/)
