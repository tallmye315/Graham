import {useEffect,useRef,useState} from 'react';
import {Button} from './ui/button';
import {Input} from './ui/input';
import {getMoneyWeighted,refreshMoneyWeighted,stopPerformanceRefresh,displayAmount,type Snapshot,type Currency,type PerformanceQuery,type MoneyWeightedState} from '../ledger';

export function MoneyWeightedPanel({snapshot}:{snapshot:Snapshot}) {
 const [account,setAccount]=useState(()=>snapshot.accounts.find(a=>a.hasHistory)?.id||'all');
 const [currency,setCurrency]=useState<Currency>('CNY');
 const [range,setRange]=useState<PerformanceQuery['range']>('month');
 const [dates,setDates]=useState({start:'',end:''});
 const [state,setState]=useState<MoneyWeightedState|null>(null);
 const [error,setError]=useState<string|null>(null);
 const [busy,setBusy]=useState(false);const generation=useRef(0);
 const query:PerformanceQuery={accountIds:account==='all'?[]:[account],currency,range,startOn:range==='custom'?dates.start:null,endOn:range==='custom'?dates.end:null};
 async function read(refresh=false) {
  const token=++generation.current;setBusy(refresh);setError(null);
  if(range==='custom'&&(!dates.start||!dates.end)){setBusy(false);setError('请选择起止日期');return;}
  try {const value=await(refresh?refreshMoneyWeighted(query):getMoneyWeighted(query));if(token===generation.current)setState(value);}
  catch(e){if(token===generation.current)setError(typeof e==='string'?e:'个人资金回报暂不可读');}
  finally{if(token===generation.current)setBusy(false);}
 }
 useEffect(()=>{setState(null);void read();return()=>{generation.current++;void stopPerformanceRefresh().catch(()=>{});};},[account,currency,range,dates.start,dates.end,snapshot]);
 useEffect(()=>{if(!state?.refresh.active||busy)return;const timer=setInterval(()=>void read(),5000);return()=>clearInterval(timer);},[state?.refresh.active,account,currency,range,dates.start,dates.end,busy,snapshot]);
 return <>
  <div className="scope">
   <label>账户 <select aria-label="资金回报账户范围" value={account} disabled={busy} onChange={e=>setAccount(e.target.value)}><option value="all">全部账户（含归档）</option>{snapshot.accounts.map(a=><option value={a.id} key={a.id}>{a.name}</option>)}</select></label>
   <label>汇总币种 <select aria-label="资金回报汇总币种" value={currency} disabled={busy} onChange={e=>setCurrency(e.target.value as Currency)}>{['CNY','USD','HKD'].map(c=><option key={c}>{c}</option>)}</select></label>
   <label>区间 <select aria-label="资金回报区间" value={range} disabled={busy} onChange={e=>setRange(e.target.value as PerformanceQuery['range'])}><option value="month">近月</option><option value="ytd">今年以来</option><option value="all">全部</option><option value="custom">自定义</option></select></label>
   {range==='custom'&&<><Input type="date" aria-label="资金回报开始日期" value={dates.start} onChange={e=>setDates({...dates,start:e.target.value})}/><Input type="date" aria-label="资金回报结束日期" value={dates.end} onChange={e=>setDates({...dates,end:e.target.value})}/></>}
  </div>
  <div className="actions"><Button disabled={busy} onClick={()=>void read(true)}>更新资金回报数据</Button><Button variant="outline" disabled={busy} onClick={()=>void read()}>读取资金回报缓存</Button>{state?.refresh.active&&<Button variant="outline" onClick={()=>void stopPerformanceRefresh().then(()=>read()).catch(e=>setError(String(e)))}>暂停历史更新</Button>}</div>
  {error&&<p role="alert" className="error-banner">{error}</p>}
  {!state?<p>{error?'暂无可核对资金回报':'正在计算个人资金回报…'}</p>:<>
   <section className="ledger-section"><h2>年化资金回报率</h2><strong>{state.annualizedReturn===null?'无法给出唯一年化回报':state.annualizedReturn+'%'}</strong><h3>区间资金回报</h3><strong>{state.periodReturn===null?'数据不足或无法计算':state.periodReturn+'%'}</strong><p>{state.startOn} — {state.endOn} · 估值基准 {state.baselineOn||'尚未建立'} · {state.elapsedDays} 天 · {state.currency}</p><p>年化 XIRR：实际日期 / 365 天；区间回报按同一年化解换算。负数为投入，正数为撤回或期末资产。</p>{state.reason&&<p role="status">{state.reason}</p>}{state.refresh.pending>0&&<p>仍有 {state.refresh.pending} 个来源区间待更新{state.refresh.retryAt?'；下次可尝试 '+state.refresh.retryAt:''}</p>}{state.refresh.error&&<p role="alert">{state.refresh.error}</p>}{state.warnings.map((w,i)=><p className="opening-note" key={i}>{w}</p>)}</section>
   <section className="ledger-section"><h2>资金回报计算明细</h2><div className="table-scroll"><table aria-label="资金回报计算明细"><thead><tr><th>日期</th><th>用途</th><th>金额 / {state.currency}</th><th>状态 / 来源说明</th></tr></thead><tbody>{state.flows.map((f,i)=><tr key={i}><td>{f.date}</td><td>{({opening:'期初资产（视为投入）',contribution:'外部投入',withdrawal:'外部撤回',closing:'期末资产（视为回收）',unavailable:'无法核对的资金流'} as Record<string,string>)[f.kind]||f.kind}</td><td>{f.amount===null?'数据不足':displayAmount(f.amount,state.currency)}</td><td>{f.reasons.length?f.reasons.map((r,j)=><small className="security-code" key={j}>{r}</small>):'已核对'}</td></tr>)}</tbody></table></div></section>
  </>}
 </>;
}
