import { useEffect, useRef, useState } from 'react';
import { Button } from './ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './ui/dialog';
import { displayAmount, getIncome, refreshIncome, type Currency, type IncomeAmounts, type IncomeRow, type IncomeState, type Snapshot } from '../ledger';

const names: Record<string, string> = { CN: 'A 股', US: '美股', HK: '港股' };
const statuses: Record<string, string> = { intraday: '盘中估算', closed: '已核对收盘', preopen: '开盘前 · 最近交易日', rest: '休市 · 最近交易日', stale: '过期行情 · 缓存估算', closing: '收盘待核对 · 参考估算', missing: '交易日待核对' };
function money(value: string | null, currency: Currency) { return value === null ? '数据不足' : displayAmount(value, currency); }
function direction(value: string | null) { return value === null ? '' : value.startsWith('-') ? 'income-loss' : value === '0.00' ? '' : 'income-gain'; }

function IncomeBreakdown({ amounts: a, currency }: { amounts: IncomeAmounts; currency: Currency }) {
  return <>
    <dl className="income-breakdown">
      <div><dt>已实现盈亏</dt><dd className={direction(a.realized)}>{money(a.realized, currency)}</dd></div>
      <div><dt>未实现盈亏</dt><dd className={direction(a.unrealized)}>{money(a.unrealized, currency)}</dd></div>
      <div><dt>税后分红</dt><dd>{money(a.netDividend, currency)}</dd></div>
    </dl>
    <details className="income-fees"><summary>分红及费用明细</summary>
      <p>待到账分红 {money(a.pendingDividend, currency)} · 已到账分红 {money(a.paidDividend, currency)}</p>
      <p>累计手续费 {money(a.fee, currency)} · 交易税 {money(a.tax, currency)} · 分红扣税 {money(a.dividendTax, currency)}</p>
      <p>本日税后分红 {money(a.todayDividend, currency)} · 手续费 {money(a.todayFee, currency)} · 交易税 {money(a.todayTax, currency)} · 分红扣税 {money(a.todayDividendTax, currency)}</p>
      <p>费用已计入成本、卖出净收入或税后分红，不再次扣除。</p>
    </details>
  </>;
}
function IncomeTable({ rows, currency, label }: { rows: IncomeRow[]; currency: Currency; label: string }) {
  return !rows.length ? <p className="income-empty">暂无证券记录</p> : <div className="table-scroll"><table aria-label={label}>
    <thead><tr><th>证券 / 账户</th><th>数量 / 剩余成本</th><th>已实现盈亏</th><th>未实现盈亏</th><th>税后分红</th><th>总盈亏</th><th>今日盈亏 / 回报率</th><th>核对明细</th></tr></thead>
    <tbody>{rows.map(row => <tr key={`${row.accountId}:${row.key.market}:${row.key.code}:${row.key.kind}`}>
      <td>{row.name}<small className="security-code">{row.key.market}:{row.key.code} · {row.accountName}</small></td>
      <td>{row.quantity}<small className="security-code">{money(row.cost, currency)}</small></td>
      <td className={direction(row.amounts.realized)}>{money(row.amounts.realized, currency)}</td>
      <td className={direction(row.amounts.unrealized)}>{money(row.amounts.unrealized, currency)}</td>
      <td>{money(row.amounts.netDividend, currency)}</td>
      <td className={direction(row.amounts.total)}>{money(row.amounts.total, currency)}</td>
      <td className={direction(row.amounts.today)}>{money(row.amounts.today, currency)}<small className="security-code">{row.amounts.dailyReturn === null ? '回报率 —' : row.amounts.dailyReturn + '%'}</small></td>
      <td className="income-row-notes"><details><summary>价格与费用</summary><IncomeBreakdown amounts={row.amounts} currency={currency}/>
        <p>来源时点：{row.sourceTime || (row.closed ? '已清仓，无需现价' : '未知')}</p><p>缓存接收：{row.receivedAt || '—'}</p></details>
        {row.reasons.map(reason => <small className="security-code income-reason" key={reason}>{reason}</small>)}</td>
    </tr>)}</tbody></table></div>;
}

export function IncomePanel({ snapshot, account }: { snapshot: Snapshot; account: string }) {
  const [state, setState] = useState<IncomeState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [closedMarket, setClosedMarket] = useState<string | null>(null);
  const generation = useRef(0);
  const inFlight = useRef(false);
  async function read(refresh = false) {
    if (inFlight.current) return;
    inFlight.current = true;
    const token = ++generation.current; setBusy(refresh); setError(null);
    try {
      const value = await (refresh ? refreshIncome(account === 'all' ? [] : [account]) : getIncome(account === 'all' ? [] : [account]));
      if (!value || !Array.isArray(value.markets)) throw '收益响应无效，请刷新或检查软件版本';
      if (token === generation.current) setState(value);
    } catch (e) {
      if (token === generation.current) setError(typeof e === 'string' ? e : '收益读取失败，请重试');
    } finally {
      if (token === generation.current) { inFlight.current = false; setBusy(false); }
    }
  }
  useEffect(() => {
    setState(null); setClosedMarket(null); generation.current++; inFlight.current = false;
    void read(); const timer = setInterval(() => void read(), 5000);
    return () => { generation.current++; inFlight.current = false; clearInterval(timer); };
  }, [snapshot, account]);
  const closed = state?.markets.find(m => m.market === closedMarket);
  return <section className="income-panel" aria-label="今日与累计证券收益">
    <div className="section-heading"><h2>证券收益</h2><Button variant="outline" disabled={busy} onClick={() => void read(true)}>{busy ? '正在刷新收益…' : '刷新今日收益'}</Button></div>
    {error && <p role="alert" className="error-banner">{error}</p>}
    {state?.cacheWarning && <p role="alert" className="error-banner">{state.cacheWarning}</p>}
    {!state ? <p className="income-empty">{error ? '暂无可核对收益' : '正在读取收益…'}</p> : <>
      <div className="income-markets">{state.markets.map(m => <section className="income-market" aria-label={`${names[m.market] || m.market}收益 · ${m.currency}`} key={m.market}>
        <header><h3>{names[m.market] || m.market} <small>{m.currency}</small></h3><span className="income-status">{statuses[m.status] || m.status}</span></header>
        <p className="income-dates">交易日 {m.tradingDate || '待核对'} · 前收盘 {m.previousDate || '待核对'}</p>
        <div className="income-primary"><div><span>今日盈亏</span><strong className={direction(m.amounts.today)}>{money(m.amounts.today, m.currency)}</strong><small>证券当日回报率 {m.amounts.dailyReturn === null ? '—' : m.amounts.dailyReturn + '%'}</small></div>
          <div><span>总盈亏</span><strong className={direction(m.amounts.total)}>{money(m.amounts.total, m.currency)}</strong><small>累计已实现＋未实现＋税后分红</small></div></div>
        <IncomeBreakdown amounts={m.amounts} currency={m.currency}/>
        {m.rows.some(r => r.reasons.length) && <p className="income-reason">存在数据缺项或参考估算，请展开明细核对。</p>}
        <div className="income-detail-actions"><details><summary>持仓与收益明细</summary><IncomeTable rows={m.rows.filter(r => !r.closed)} currency={m.currency} label={`${names[m.market]}持仓收益明细`}/></details>
          <Button variant="ghost" aria-label={`查看${names[m.market]}已清仓证券`} onClick={() => setClosedMarket(m.market)}>已清仓证券（{m.rows.filter(r => r.closed).length}）</Button></div>
      </section>)}</div>
      <details className="income-method"><summary>收益口径</summary><p>{state.method}</p><p>各市场、各币种独立核算；闲置现金单列。盘中自动读取缓存，来源抓取频率受行情权限与退避限制，报价时间见明细。</p></details>
    </>}
    <Dialog open={!!closed} onOpenChange={open => { if (!open) setClosedMarket(null); }}><DialogContent className="entry-dialog income-closed-dialog"><DialogHeader><DialogTitle>已清仓证券 · {closed ? names[closed.market] : ''} · {closed?.currency}</DialogTitle><DialogDescription>已实现盈亏与税后分红分别展示；已清仓收益仍计入对应市场的累计结果。</DialogDescription></DialogHeader>
      {closed && <IncomeTable rows={closed.rows.filter(r => r.closed)} currency={closed.currency} label={`${names[closed.market]}已清仓收益明细`}/>}
      <div className="form-actions"><Button variant="outline" aria-label="关闭已清仓窗口" onClick={() => setClosedMarket(null)}>关闭</Button></div>
    </DialogContent></Dialog>
  </section>;
}
