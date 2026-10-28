# Graham 统一 CSV v1

在“CSV 导入”查看账户 ID，选择 UTF-8 CSV 或粘贴文本，再预览和确认。CSV 是统一格式，不直接猜测华龙或兴业的原始导出列；专用适配由后续任务交付。必须已有对应原币账户，不自动建账。最多 2 MiB / 2000 数据行。

必填表头为 `record_id,type,account_id,date,currency`，各行使用 `YYYY-MM-DD` 和 `CNY/USD/HKD`。record_id 是该账户内稳定且唯一的外部事件号。同一日按文件行序核算（入金优先）；新文件包含整批依赖时一次校验，不部分提交。

| type | 额外字段 |
| --- | --- |
| deposit / withdrawal | amount：入金/出金正金额 |
| buy / sell | market,code,kind,name,quantity,price,fee,tax |
| dividend | market,code,kind,name,amount（税前）,tax,net,status（pending/paid）,paid_on（paid 必填） |
| transfer / exchange | amount,to_account_id,to_currency,to_amount,rate,fee,fee_side（from/to） |
| opening | amount（收盘现金）,positions（持仓 JSON 数组，可空 `[]`） |

所有类型可填 note。空 fee/tax 默认为 0，fee_side 默认为 from。金额最多两位、数量及单价六位、rate 八位小数，使用十进制，不使用千位分隔符。market 为 CN/HK/US，代码为六位 A 股、五位港股或大写美股代码，kind 为 stock/etf。证券及账户原币一致；换汇本金不含手续费，汇率为 1 转出币对应的转入币。

positions 每项为 market,code,kind,name,currency,quantity,cost,referencePrice；cost 是已核对剩余总成本，referencePrice 可为 null。CSV 字段包含逗号、换行或双引号时必须用双引号包围，内部双引号写成两个双引号。

例如先把 ACCOUNT_ID 替换为真实人民币账户 ID：

```csv
record_id,type,account_id,date,currency,amount,note
event-001,deposit,ACCOUNT_ID,2026-10-01,CNY,1000,已核对的实际入金
event-002,withdrawal,ACCOUNT_ID,2026-10-02,CNY,100,已核对的实际出金
```

重复判断使用账户 ID + record_id，数字 100 与 100.00 等值；内容与该身份的原始记录不符时显示冲突，不能靠重新导入覆盖。错误行及未知类型必须修正后再预览。手动记录没有外部事件号，不猜测与 CSV 的对应关系，导入前需自行核对已有手工登记。

在导入批次来源中修改一条记录、恢复/删除记录，或先预览再撤回整批。不能改 record_id、记录类型或两端账户；错误身份先作废，再以新事件号重录。全部操作重算后续结果，超卖或现金不足则整次拒绝。撤回/删除保留原始文件、身份、历史与去重结果；导入记录不提供彻底删除。
