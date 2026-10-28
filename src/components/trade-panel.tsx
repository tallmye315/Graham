import { useRecordSession } from '../record-session';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './ui/dialog';
import { displayAmount, openingCovers, loadLedger, recordTrade, reviseTrade, setTradeDeleted, permanentlyDeleteTrade, tradeHistory, restoreTradeRevision, valuePosition, type Snapshot, type TradeInput, type Trade, type Position, type PositionValue, type QuoteState, type QuoteView, type QuotedHolding } from '../ledger';

const markets = { CN: 'A 股', HK: '港股', US: '美股' };
const marketCurrency = { CN: 'CNY', HK: 'HKD', US: 'USD' } as const;
function today() { const d = new Date(); return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`; }

function PositionRow({ position: p, account, quote, valuation }: { position: Position; account: string; quote?:QuoteView; valuation?:QuotedHolding }) {
  const [price, setPrice] = useState('');
  const [value, setValue] = useState<PositionValue | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const request = useRef(0);
  useEffect(() => { request.current++; setValue(null); setError(null); return () => { request.current++; }; }, [p]);
  async function calculate(event: FormEvent) {
    event.preventDefault(); if (busy) return;
    const current = ++request.current;
    setBusy(true); setValue(null); setError(null);
    try { const result = await valuePosition(p.accountId, p.market, p.code, price); if (current === request.current) setValue(result); }
    catch (e) { if (current === request.current) setError(typeof e === 'string' ? e : '试算失败，请刷新后重试'); }
    finally { setBusy(false); }
  }
  return <tr><td><div className="security-cell"><div><strong>{p.name}</strong><small className="security-code">{p.market}:{p.code} · {p.kind === 'etf' ? 'ETF' : '股票'}</small></div></div></td><td>{account}</td><td>{p.quantity}</td><td>{displayAmount(p.cost, p.currency)}</td><td>{p.quantity === '0' ? '—' : `${p.averageCost} ${p.currency}`}</td><td>{displayAmount(p.realized, p.currency)}{p.since&&<small className="security-code">{p.since} 期初以来</small>}</td><td>{quote&&<div className="opening-note">行情 {quote.price===null?'数据不足':`${quote.price} ${quote.currency}`}<small className="security-code">{quote.source} · {quote.quotedAt||'完整报价日期未知'}{quote.error?` · ${quote.error}`:''}</small>{valuation&&valuation.quantity===p.quantity&&valuation.cost===p.cost&&<small className="security-code">市值 {valuation.marketValue===null?'数据不足':displayAmount(valuation.marketValue,p.currency)} · 浮盈亏 {valuation.unrealized===null?'数据不足':displayAmount(valuation.unrealized,p.currency)}</small>}</div>}<form className="valuation-form" onSubmit={event => void calculate(event)}><Input aria-label={`${p.code} 手动试算价格`} inputMode="decimal" placeholder="参考价格" value={price} disabled={busy} onChange={e => { setPrice(e.target.value); setValue(null); }} /><Button size="sm" variant="outline" disabled={busy || !price}>试算</Button></form>{value && <small className="security-code">手动试算：市值 {displayAmount(value.marketValue, p.currency)} · 浮盈亏 {displayAmount(value.unrealized, p.currency)}</small>}{error && <small role="alert">{error}</small>}</td></tr>;
}

export function TradePanel({ snapshot, busy, onBusyChange, onChanged, market:quotesState }: {market?:QuoteState|null; snapshot: Snapshot; busy: boolean; onBusyChange: (value: boolean) => void; onChanged: (snapshot: Snapshot) => void }) {
  const [account, setAccount] = useState('all');
  const [deleted, setDeleted] = useState(false);
  const [dialog, setDialog] = useState<'form' | 'delete' | 'history' | null>(null);
  const [editing, setEditing] = useState<Trade | null>(null);
  const [market, setMarket] = useState<TradeInput['market']>('CN');



  const activeAccounts = snapshot.accounts.filter(a => !a.archived);
  const trades = (snapshot.trades || []).filter(t => (account === 'all' || t.input.accountId === account) && t.deleted === deleted).slice().reverse();
  const positions = (snapshot.positions || []).filter(p => account === 'all' || p.accountId === account);
  const accounts = snapshot.accounts.filter(a => account === 'all' || a.id === account);



  const { error, setError, history, undo, change } = useRecordSession({
   busy, onBusyChange, onChanged, records: result => result.trades || [],
   historyRecord: dialog === 'history' ? editing : null, historyLoader: tradeHistory,
   onSuccess: () => { setDialog(null); setEditing(null); },
 });
 function openForm(trade: Trade | null) {
    setEditing(trade); setMarket(trade?.input.market || 'CN'); setError(null); setDialog('form');
  }
  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); const data = new FormData(event.currentTarget);
    const input: TradeInput = { accountId: editing?.input.accountId || String(data.get('account')), occurredOn: String(data.get('date')), market, code: String(data.get('code')).trim().toUpperCase(), kind: data.get('kind') as TradeInput['kind'], name: String(data.get('name')).trim(), currency: marketCurrency[market], side: data.get('side') as TradeInput['side'], quantity: String(data.get('quantity')), price: String(data.get('price')), fee: String(data.get('fee')), tax: String(data.get('tax')), note: String(data.get('note') || '') };
    await change(() => editing ? reviseTrade(editing.id, editing.revisionId, input) : recordTrade(input), editing || undefined);
  }
  return <>
    <div className="scope"><select aria-label="买卖账户筛选" value={account} disabled={busy} onChange={e => setAccount(e.target.value)}><option value="all">全部账户</option>{snapshot.accounts.map(a => <option key={a.id} value={a.id}>{a.name}</option>)}</select><div className="actions"><Button variant="outline" disabled={busy} onClick={() => void change(loadLedger)}>刷新记录</Button><Button disabled={busy || !activeAccounts.length} onClick={() => openForm(null)}>录入买卖</Button></div></div>
    {error && !dialog && <p className="error-banner" role="alert">{error}</p>}
    {undo && <div className="undo-banner">交易已更新<Button size="sm" variant="outline" disabled={busy} onClick={() => void change(() => restoreTradeRevision(undo.id, undo.current, undo.previous))}>撤回交易修改</Button></div>}
    {snapshot.openings?.filter(o=>!o.deleted&&(account==='all'||o.input.accountId===account)).map(o=><p className="opening-coverage" key={o.id}>{snapshot.accounts.find(a=>a.id===o.input.accountId)?.name}：从 {o.input.occurredOn} 收盘期初核算，此前收益数据不足。</p>)}
    <div className="trade-cash">{accounts.map(a => <span key={a.id}>{a.name} · 现金 <strong>{displayAmount(a.balance, a.currency)}</strong></span>)}</div>
    <section className="ledger-section"><h2>持仓与成本</h2>{!positions.length ? <p className="no-records">暂无持仓，录入买入后显示成本。</p> : <div className="table-scroll"><table><thead><tr><th>证券 / 唯一代码</th><th>账户</th><th>数量</th><th>剩余成本 / 原币</th><th>平均单位成本</th><th>已实现盈亏 / 原币</th><th>行情估值 / 手动试算</th></tr></thead><tbody>{positions.map(p => <PositionRow key={`${p.accountId}:${p.market}:${p.code}`} quote={quotesState?.quotes.find(q=>q.key.market===p.market&&q.key.code===p.code&&q.key.kind===p.kind)} valuation={quotesState?.holdings.find(h=>h.accountId===p.accountId&&h.key.market===p.market&&h.key.code===p.code&&h.key.kind===p.kind)} position={p} account={snapshot.accounts.find(a => a.id === p.accountId)?.name || ''} />)}</tbody></table></div>}</section>
    <section className="ledger-section"><div className="section-heading"><h2>买卖记录</h2><select aria-label="买卖记录状态" disabled={busy} value={deleted ? 'deleted' : 'active'} onChange={e => setDeleted(e.target.value === 'deleted')}><option value="active">有效记录</option><option value="deleted">已删除</option></select></div>{!trades.length ? <p className="no-records">暂无{deleted ? '已删除' : '有效'}买卖记录</p> : <div className="table-scroll"><table><thead><tr><th>日期 / 账户</th><th>证券</th><th>方向</th><th>数量 × 单价</th><th>费用 / 税款</th><th>来源 / 备注</th><th>操作</th></tr></thead><tbody>{trades.map(t => <tr key={t.id}><td>{t.input.occurredOn}<small className="security-code">{snapshot.accounts.find(a => a.id === t.input.accountId)?.name}</small></td><td><div className="security-cell"><div>{t.input.name}<small className="security-code">{t.input.market}:{t.input.code} · {t.input.kind === 'etf' ? 'ETF' : '股票'}</small></div></div></td><td>{t.input.side === 'buy' ? '买入' : '卖出'}</td><td>{t.input.quantity} × {t.input.price} {t.input.currency}</td><td>{t.input.fee} / {t.input.tax} {t.input.currency}</td><td>{t.source}{openingCovers(snapshot,t.input.accountId,t.input.occurredOn)&&<small className="security-code">期初已覆盖，不重复计入</small>}<small className="security-code">{t.input.note || '—'}</small></td><td className="row-actions">{t.deleted ? <><Button size="sm" variant="ghost" disabled={busy} onClick={() => void change(() => setTradeDeleted(t.id, t.revisionId, false), t)}>恢复交易</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void change(() => permanentlyDeleteTrade(t.id, t.revisionId))}>彻底删除交易</Button></> : <><Button size="sm" variant="ghost" disabled={busy} onClick={() => openForm(t)}>修改交易</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => { setEditing(t); setError(null); setDialog('delete'); }}>删除交易</Button></>}<Button size="sm" variant="ghost" disabled={busy} onClick={() => { setEditing(t); setError(null); setDialog('history'); }}>交易历史</Button></td></tr>)}</tbody></table></div>}</section>
    <Dialog open={dialog !== null} onOpenChange={open => { if (!open && !busy) { setDialog(null); setError(null); } }}><DialogContent className="entry-dialog trade-dialog"><DialogHeader><DialogTitle>{dialog === 'delete' ? '删除这笔交易？' : dialog === 'history' ? '交易历史' : editing ? '修改买卖' : '录入买卖'}</DialogTitle><DialogDescription>{dialog === 'delete' ? '删除可恢复。若影响后续卖出或现金，操作会被阻止，请先核对相关记录。' : dialog === 'history' ? '恢复版本会重新核对后续交易及现金。' : '仅记录已成交交易，不会下单。数量和单价最多六位小数，税费最多两位小数。同日交易请按实际成交先后录入。'}</DialogDescription></DialogHeader>
      {dialog === 'form' && <form className="entry-form" onSubmit={event => void save(event)}>
        <Label htmlFor="trade-account">投资账户</Label><select id="trade-account" name="account" disabled={busy || !!editing} defaultValue={editing?.input.accountId || (account !== 'all' ? account : undefined)}>{(editing ? snapshot.accounts.filter(a => a.id === editing.input.accountId) : activeAccounts).map(a => <option key={a.id} value={a.id}>{a.name} · {a.currency}</option>)}</select>
        <div className="trade-fields"><div><Label htmlFor="trade-date">成交日期</Label><Input id="trade-date" name="date" type="date" required disabled={busy} defaultValue={editing?.input.occurredOn || today()} /></div><div><Label htmlFor="trade-side">交易方向</Label><select id="trade-side" name="side" disabled={busy} defaultValue={editing?.input.side || 'buy'}><option value="buy">买入</option><option value="sell">卖出</option></select></div>
          <div><Label htmlFor="trade-market">证券市场</Label><select id="trade-market" value={market} disabled={busy} onChange={e => setMarket(e.target.value as TradeInput['market'])}>{Object.entries(markets).map(([key,name]) => <option key={key} value={key}>{name} · {marketCurrency[key as TradeInput['market']]}</option>)}</select></div><div><Label htmlFor="trade-kind">证券类型</Label><select id="trade-kind" name="kind" disabled={busy} defaultValue={editing?.input.kind || 'stock'}><option value="stock">股票</option><option value="etf">ETF</option></select></div>
          <div><Label htmlFor="trade-code">证券代码</Label><Input id="trade-code" name="code" required maxLength={20} disabled={busy} defaultValue={editing?.input.code} placeholder={market === 'CN' ? '600036' : market === 'HK' ? '00700' : 'AAPL'} /></div><div><Label htmlFor="trade-name">证券名称</Label><Input id="trade-name" name="name" required maxLength={80} disabled={busy} defaultValue={editing?.input.name} /></div>
          <div><Label htmlFor="trade-quantity">数量 / 股或份</Label><Input id="trade-quantity" name="quantity" inputMode="decimal" required disabled={busy} defaultValue={editing?.input.quantity} /></div><div><Label htmlFor="trade-price">成交单价 / 原币</Label><Input id="trade-price" name="price" inputMode="decimal" required disabled={busy} defaultValue={editing?.input.price} /></div>
          <div><Label htmlFor="trade-fee">实际费用 / 原币</Label><Input id="trade-fee" name="fee" inputMode="decimal" required disabled={busy} defaultValue={editing?.input.fee || '0'} /></div><div><Label htmlFor="trade-tax">实际税款 / 原币</Label><Input id="trade-tax" name="tax" inputMode="decimal" required disabled={busy} defaultValue={editing?.input.tax || '0'} /></div></div>
        <Label htmlFor="trade-note">备注</Label><Input id="trade-note" name="note" maxLength={500} disabled={busy} defaultValue={editing?.input.note} />
        {error && <p role="alert" className="form-error">{error}</p>}<div className="form-actions"><Button type="button" variant="outline" disabled={busy} onClick={() => setDialog(null)}>取消</Button><Button disabled={busy}>保存交易</Button></div>
      </form>}
      {dialog === 'delete' && editing && <><p>{editing.input.name} · {editing.input.quantity} · {editing.input.occurredOn}</p>{error && <p role="alert">{error}</p>}<Button disabled={busy} onClick={() => void change(() => setTradeDeleted(editing.id, editing.revisionId, true), editing)}>确认删除交易</Button></>}
      {dialog === 'history' && editing && <div className="history-list">{error && <p role="alert">{error}</p>}{!history && !error && <p>正在读取…</p>}{history?.map(r => <div className="history-item" key={r.id}><time>{r.changedAt} · {r.action}{r.deleted ? ' · 已删除' : ''}</time><p>{r.input.market}:{r.input.code} · {r.input.side === 'buy' ? '买入' : '卖出'} {r.input.quantity} × {r.input.price} {r.input.currency} · 费用 {r.input.fee} · 税 {r.input.tax}</p><p>{r.input.note}</p><Button size="sm" variant="outline" disabled={busy || r.id === editing.revisionId} onClick={() => void change(() => restoreTradeRevision(editing.id, editing.revisionId, r.id), editing)}>恢复此交易版本</Button></div>)}</div>}
    </DialogContent></Dialog>
  </>;
}
