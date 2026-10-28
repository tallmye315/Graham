import { version } from '../../package.json';

export function ReleasePanel() {
  return <>
    <section className="ledger-section"><h2>Graham {version}</h2><div className="release-copy">
      <p>中文 Windows 个人投资账本 · 本地保存 · 手动升级</p>
      <p>支持手动登记、统一 CSV 导入、行情估值、收益曲线、资金回报和组合分析。华龙证券、兴业证券专用文件格式尚未适配；请按统一 CSV 格式整理后导入。</p>
      <p>行情刷新不会同步券商交易。免费数据可能延时、缺失或暂不可用；报价时间与软件接收时间分别展示，缺项不按零计算。A 股、港股使用 AKShare，美股使用 yfinance。</p>
      <p>期初之前、缺少历史价格或汇率的区间无法还原收益。拆股等公司行动尚无自动入账流程；发现数量或历史价格不一致时，先核对记录。行业是本地业务归纳，未知项保留未分类，基金未穿透。</p>
    </div></section>
    <section className="ledger-section"><h2>本地数据与升级</h2><div className="release-copy">
      <p>升级前保存正在编辑的内容并关闭 Graham，建议先从“备份与恢复”导出加密备份。运行新版安装包即可升级，账本与行情缓存位于用户数据目录，独立于安装目录。</p>
      <p>账本迁移前自动保存本地快照；遇到失败请保留原账本及快照，不要删除数据目录。加密备份通过“备份与恢复”验证后整体恢复，软件不保存备份密码。</p>
      <p>当前本机账本未加密；独立导出的备份使用密码加密。彻底删除不能在软件内撤回，已有备份和升级快照不会随之清除。</p>
    </div></section>
  </>;
}
