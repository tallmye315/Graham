import { useRecordSession } from '../record-session';
import {useEffect,useState,type FormEvent} from 'react';
import {Button} from './ui/button';
import {Input} from './ui/input';
import {Label} from './ui/label';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from './ui/dialog';
import {displayAmount,saveOpening,setOpeningDeleted,restoreOpeningRevision,permanentlyDeleteOpening,openingHistory,snapshotOn,loadLedger,type Snapshot,type Opening,type OpeningPosition,type DatedSnapshot,type Currency} from '../ledger';

const currencies={CN:'CNY',HK:'HKD',US:'USD'} as const;
function today(){const d=new Date();return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;}
export function OpeningPanel({snapshot,busy,onBusyChange,onChanged}:{snapshot:Snapshot;busy:boolean;onBusyChange:(busy:boolean)=>void;onChanged:(snapshot:Snapshot)=>void}){
 const [account,setAccount]=useState('all'); const [deleted,setDeleted]=useState(false);
 const [dialog,setDialog]=useState<'form'|'delete'|'history'|null>(null);
 const [editing,setEditing]=useState<Opening|null>(null);const [formAccount,setFormAccount]=useState('');
 const [date,setDate]=useState(today);const [cash,setCash]=useState('0');const [positions,setPositions]=useState<OpeningPosition[]>([]);const [note,setNote]=useState('');
 const [confirmed,setConfirmed]=useState(false);

 const [queryDate,setQueryDate]=useState(today);const [dated,setDated]=useState<DatedSnapshot|null>(null);
 const openings=snapshot.openings||[];const available=snapshot.accounts.filter(a=>!a.archived&&!openings.some(o=>o.input.accountId===a.id));
 const visible=openings.filter(o=>o.deleted===deleted&&(account==='all'||account===o.input.accountId));
 const currency=snapshot.accounts.find(a=>a.id===formAccount)?.currency||'CNY';
 const covered=[...snapshot.entries.map(e=>({accountId:e.accountId,date:e.occurredOn,deleted:e.deleted})),...(snapshot.trades||[]).map(t=>({accountId:t.input.accountId,date:t.input.occurredOn,deleted:t.deleted}))].filter(r=>!r.deleted&&r.accountId===formAccount&&r.date<=date).length;
 useEffect(()=>{setDated(null);},[snapshot]);

 const { error, setError, history, undo, change } = useRecordSession({
   busy, onBusyChange, onChanged, records: result => result.openings || [],
   historyRecord: dialog === 'history' ? editing : null, historyLoader: openingHistory,
   onSuccess: () => { setDialog(null); setEditing(null); },
 });
 function openForm(o:Opening|null){setEditing(o);setFormAccount(o?.input.accountId||available.find(a=>a.id===account)?.id||available[0]?.id||'');setDate(o?.input.occurredOn||today());setCash(o?.input.cash||'0');setPositions(o?.input.positions.map(p=>({...p}))||[]);setNote(o?.input.note||'');setConfirmed(false);setError(null);setDialog('form');}
 function updatePosition(index:number,patch:Partial<OpeningPosition>){setPositions(rows=>rows.map((p,i)=>i===index?{...p,...patch}:p));setConfirmed(false);}
 function addPosition(){const market=currency==='USD'?'US':currency==='HKD'?'HK':'CN';setPositions(rows=>[...rows,{market,code:'',kind:'stock',name:'',currency,quantity:'',cost:'',referencePrice:null}]);setConfirmed(false);}
 async function save(e:FormEvent){e.preventDefault();if(!confirmed)return;await change(()=>saveOpening({accountId:formAccount,occurredOn:date,cash,positions:positions.map(p=>({...p,code:p.code.trim().toUpperCase(),name:p.name.trim()})),note},editing?.revisionId??null),editing||undefined);}
 async function query(e:FormEvent){e.preventDefault();if(busy)return;onBusyChange(true);setError(null);setDated(null);try{setDated(await snapshotOn(queryDate));}catch(e){setError(typeof e==='string'?e:'历史查询失败');}finally{onBusyChange(false);}}
 return <>
  <div className="scope"><select aria-label="期初账户筛选" value={account} disabled={busy} onChange={e=>setAccount(e.target.value)}><option value="all">全部账户</option>{snapshot.accounts.map(a=><option key={a.id} value={a.id}>{a.name}</option>)}</select><div className="actions"><Button variant="outline" disabled={busy} onClick={()=>void change(loadLedger)}>刷新记录</Button><Button disabled={busy||!available.length} onClick={()=>openForm(null)}>建立期初快照</Button></div></div>
  {error&&!dialog&&<p role="alert" className="error-banner">{error}</p>}
  {undo&&<div className="undo-banner">期初快照已更新<Button size="sm" variant="outline" disabled={busy} onClick={()=>void change(()=>restoreOpeningRevision(undo.id,undo.current,undo.previous))}>撤回期初修改</Button></div>}
  <section className="ledger-section"><div className="section-heading"><h2>账户核对起点</h2><select aria-label="期初快照状态" value={deleted?'deleted':'active'} disabled={busy} onChange={e=>setDeleted(e.target.value==='deleted')}><option value="active">有效快照</option><option value="deleted">已删除</option></select></div>
   {!visible.length?<p className="no-records">暂无{deleted?'已删除':'有效'}期初快照。历史记录不完整时，可从核对后的现金和持仓开始。</p>:visible.map(o=>{const a=snapshot.accounts.find(a=>a.id===o.input.accountId)!;return <article className="opening-card" key={o.id}>
    <div className="section-heading"><div><strong>{a.name}</strong><p className="security-code">{o.input.occurredOn} 收盘后</p></div><span>现金 {displayAmount(`${o.input.cash.trim().split('.')[0]}.${(o.input.cash.trim().split('.')[1]||'').padEnd(2,'0')}`,a.currency)}</span></div>
    <p className="opening-note">{o.deleted?'已删除，当前按原有记录核算。':`${o.input.occurredOn} 收盘前（含当日）的记录已覆盖，不重复计入；此前收益数据不足。`} · {o.source}</p>
    {o.input.positions.length>0&&<div className="table-scroll"><table><thead><tr><th>证券</th><th>数量</th><th>剩余成本 / 原币</th><th>期初参考单价 / 原币</th></tr></thead><tbody>{o.input.positions.map(p=><tr key={`${p.market}:${p.code}`}><td><div className="security-cell"><span>{p.name}<small className="security-code">{p.market}:{p.code} · {p.kind==='etf'?'ETF':'股票'}</small></span></div></td><td>{p.quantity}</td><td>{p.cost} {p.currency}</td><td>{p.referencePrice?`${p.referencePrice} ${p.currency}`:'未提供，期初估值数据不足'}</td></tr>)}</tbody></table></div>}
    {o.input.note&&<p className="opening-note">{o.input.note}</p>}
    <div className="opening-actions">{o.deleted?<><Button size="sm" variant="outline" disabled={busy} onClick={()=>void change(()=>setOpeningDeleted(o.id,o.revisionId,false),o)}>恢复期初快照</Button><Button size="sm" variant="outline" disabled={busy} onClick={()=>void change(()=>permanentlyDeleteOpening(o.id,o.revisionId))}>彻底删除期初快照</Button></>:<><Button size="sm" variant="outline" disabled={busy} onClick={()=>openForm(o)}>修改期初快照</Button><Button size="sm" variant="outline" disabled={busy} onClick={()=>{setEditing(o);setError(null);setDialog('delete');}}>删除期初快照</Button></>}<Button size="sm" variant="ghost" disabled={busy} onClick={()=>{setEditing(o);setError(null);setDialog('history');}}>期初历史</Button></div>
   </article>;})}
  </section>
  <section className="backup-card"><h2>历史账本核对</h2><form className="historical-query" onSubmit={e=>void query(e)}><Label htmlFor="history-date">历史核对日期</Label><Input id="history-date" type="date" required value={queryDate} disabled={busy} onChange={e=>{setQueryDate(e.target.value);setDated(null);}}/><Button disabled={busy}>核对当日账本</Button></form>
   {dated&&<div className="dated-results">{dated.accounts.filter(a=>account==='all'||a.accountId===account).map(a=><div key={a.accountId}><strong>{a.name} · {dated.date} 收盘</strong>{a.unavailableReason?<p>{a.unavailableReason}</p>:<><p>现金 {displayAmount(a.balance!,a.currency)}</p>{a.positions.map(p=><p key={`${p.market}:${p.code}`}>{p.name} · {p.quantity} 股/份 · 成本 {displayAmount(p.cost,a.currency)} · {p.since?`${p.since}期初以来`:'已记录区间'}已实现 {displayAmount(p.realized,a.currency)}</p>)}</>}</div>)}</div>}
  </section>
  <Dialog open={dialog!==null} onOpenChange={open=>{if(!open&&!busy){setDialog(null);setError(null);}}}><DialogContent className="entry-dialog opening-dialog"><DialogHeader><DialogTitle>{dialog==='form'?editing?'修改期初快照':'建立期初快照':dialog==='delete'?'删除期初快照？':'期初快照历史'}</DialogTitle><DialogDescription>{dialog==='form'?'录入经核对的当日收盘现金、数量和剩余成本。起点不属于新增入金或买入，不反推此前收益。':dialog==='delete'?'删除后恢复原有记录时间线；若导致后续超卖或现金不足，将拒绝操作。历史保留，可以恢复。':'每次修改、删除和恢复均保留，可恢复旧版本；仍需通过完整交易依赖检查。'}</DialogDescription></DialogHeader>
   {dialog==='form'&&<form className="entry-form" onSubmit={e=>void save(e)}>
    <Label htmlFor="opening-account">投资账户</Label><select id="opening-account" value={formAccount} disabled={busy||!!editing} onChange={e=>{setFormAccount(e.target.value);setPositions([]);setConfirmed(false);}}>{(editing?snapshot.accounts.filter(a=>a.id===editing.input.accountId):available).map(a=><option key={a.id} value={a.id}>{a.name} · {a.currency}</option>)}</select>
    <div className="trade-fields"><div><Label htmlFor="opening-date">核对日期（收盘后）</Label><Input id="opening-date" type="date" required value={date} disabled={busy} onChange={e=>{setDate(e.target.value);setConfirmed(false);}}/></div><div><Label htmlFor="opening-cash">期初现金 / 原币</Label><Input id="opening-cash" inputMode="decimal" required value={cash} disabled={busy} onChange={e=>{setCash(e.target.value);setConfirmed(false);}}/></div></div>
    <div className="section-heading"><h3>期初持仓</h3><Button type="button" size="sm" variant="outline" disabled={busy||positions.length>=1000} onClick={addPosition}>添加持仓</Button></div>
    {positions.map((p,i)=><div className="opening-position-form" key={i}><div className="trade-fields">
     <div><Label htmlFor={`opening-market-${i}`}>证券市场 {i+1}</Label><select id={`opening-market-${i}`} value={p.market} disabled={busy} onChange={e=>{const market=e.target.value as OpeningPosition['market'];updatePosition(i,{market,currency:currencies[market]});}}><option value="CN">A 股 · CNY</option><option value="HK">港股 · HKD</option><option value="US">美股 · USD</option></select></div><div><Label htmlFor={`opening-kind-${i}`}>证券类型 {i+1}</Label><select id={`opening-kind-${i}`} disabled={busy} value={p.kind} onChange={e=>updatePosition(i,{kind:e.target.value as OpeningPosition['kind']})}><option value="stock">股票</option><option value="etf">ETF</option></select></div>
     {([['code','证券代码'],['name','证券名称'],['quantity','数量 / 股或份'],['cost','已核对剩余成本 / 原币']] as const).map(([field,label])=><div key={field}><Label htmlFor={`opening-${field}-${i}`}>{label} {i+1}</Label><Input id={`opening-${field}-${i}`} required disabled={busy} value={p[field]} maxLength={field==='name'?80:field==='code'?20:40} onChange={e=>updatePosition(i,{[field]:e.target.value})}/></div>)}
     <div><Label htmlFor={`opening-price-${i}`}>期初参考单价（选填） {i+1}</Label><Input id={`opening-price-${i}`} inputMode="decimal" disabled={busy} value={p.referencePrice||''} onChange={e=>updatePosition(i,{referencePrice:e.target.value||null})}/></div>
    </div><Button type="button" size="sm" variant="ghost" disabled={busy} onClick={()=>{setPositions(rows=>rows.filter((_,j)=>i!==j));setConfirmed(false);}}>移除此持仓 {i+1}</Button></div>)}
    <Label htmlFor="opening-note">核对备注</Label><Input id="opening-note" maxLength={500} disabled={busy} value={note} onChange={e=>{setNote(e.target.value);setConfirmed(false);}}/>
    <p className="opening-note">{covered} 条有效记录被此起点覆盖，仍保留原始内容和历史，不重复计入现金、持仓或投入。后续交易从下一日开始计算。剩余成本须经过核对；参考价格未提供时，期初估值标为数据不足。</p>
    <label className="restore-confirmation"><input type="checkbox" disabled={busy} checked={confirmed} onChange={e=>setConfirmed(e.target.checked)}/>我已核对收盘状态，并确认该日及以前记录不重复计入</label>
    {error&&<p role="alert" className="form-error">{error}</p>}<div className="form-actions"><Button type="button" variant="outline" disabled={busy} onClick={()=>setDialog(null)}>取消</Button><Button disabled={busy||!confirmed}>保存期初快照</Button></div>
   </form>}
   {dialog==='delete'&&editing&&<form className="entry-form" onSubmit={e=>{e.preventDefault();if(!busy)void change(()=>setOpeningDeleted(editing.id,editing.revisionId,true),editing);}}>
    <p>{snapshot.accounts.find(a=>a.id===editing.input.accountId)?.name} · 1 条 · {editing.input.occurredOn} 收盘后</p>

    {error&&<><p role="alert">{error}</p><Button type="button" variant="outline" disabled={busy} onClick={()=>void change(loadLedger)}>刷新并重新选择</Button></>}
    <div className="form-actions"><Button type="button" variant="outline" disabled={busy} onClick={()=>setDialog(null)}>取消</Button><Button disabled={busy}>确认删除期初</Button></div>
   </form>}
   {dialog==='history'&&editing&&<div className="history-list">{error&&<p role="alert">{error}</p>}{!history&&!error&&<p>正在读取…</p>}{history?.map(r=><div className="history-item" key={r.id}><time>{r.changedAt} · {r.action}{r.deleted?' · 已删除':''}</time><p>{r.input.occurredOn} 收盘后 · 现金 {r.input.cash} · {r.input.positions.length} 项持仓</p><p>{r.input.note}</p>{r.input.positions.map(p=><p key={`${p.market}:${p.code}`}>{p.market}:{p.code} · {p.quantity} · 成本 {p.cost} {p.currency} · 参考价 {p.referencePrice??'未提供'}</p>)}<Button size="sm" variant="outline" disabled={busy||r.id===editing.revisionId} onClick={()=>void change(()=>restoreOpeningRevision(editing.id,editing.revisionId,r.id),editing)}>恢复此期初版本</Button></div>)}</div>}
  </DialogContent></Dialog>
 </>;
}
