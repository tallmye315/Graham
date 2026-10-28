use super::{amount_text,backup,database_error,trades,valuation,DailyClose,Ledger,QuoteKey,DatedAccount,ValuationJob,ValuationPoint,ValuationQuery,ValuationSource};
use chrono::{DateTime,Duration,Months,NaiveDate,Timelike,Utc};
use rusqlite::{params,OptionalExtension,TransactionBehavior};
use std::collections::{BTreeMap,BTreeSet};
use uuid::Uuid;
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct PerformanceQuery{pub account_ids:Vec<String>,pub currency:String,pub range:String,pub start_on:Option<String>,pub end_on:Option<String>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct PerformancePoint{pub date:String,pub value:Option<String>,pub inflow:Option<String>,pub outflow:Option<String>,pub daily_return:Option<String>,pub cumulative_return:Option<String>,pub status:String,pub reasons:Vec<String>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct PerformanceRefresh{pub pending:usize,pub retry_at:Option<String>,pub error:Option<String>,pub active:bool}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct PerformanceState{pub start_on:String,pub end_on:String,pub currency:String,pub baseline_on:Option<String>,pub total_return:Option<String>,pub complete:bool,pub method:String,pub points:Vec<PerformancePoint>,pub warnings:Vec<String>,pub refresh:PerformanceRefresh}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct PerformanceJob{pub ticket:String,pub start_on:String,pub end_on:String,pub keys:Vec<QuoteKey>}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct HistorySeries{pub key:QuoteKey,pub source:String,pub basis:String,pub timezone:String,pub prices:Vec<DailyClose>,pub invalidates_before:Option<String>,pub error:Option<String>}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct HistorySession{pub date:String,pub session:Option<String>,pub reason:Option<String>}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct HistoryCalendar{pub market:String,pub name:String,pub version:String,pub sessions:Vec<HistorySession>}
#[derive(Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct HistoryResponse{pub feed:String,pub series:Vec<HistorySeries>,pub calendars:Vec<HistoryCalendar>,pub error:Option<String>}

const UNIT:i64=1_000_000_000_000;
const FEED:&str="performance:history";
fn timestamp(now:i64)->Result<String,String>{DateTime::<Utc>::from_timestamp(now,0).map(|d|d.to_rfc3339()).ok_or_else(||"系统时间无效".into())}
fn day(value:&str)->Result<NaiveDate,String>{let d=NaiveDate::parse_from_str(value,"%Y-%m-%d").map_err(|_|"日期无效")?;if d.to_string()!=value{return Err("日期无效".into());}Ok(d)}
fn market(key:&QuoteKey)->&str{if key.kind=="fx"{"FX"}else{&key.market}}
fn calendar(m:&str)->(&str,&str){match m{"CN"=>("XSHG","4.13.2"),"HK"=>("XHKG","4.13.2"),"US"=>("XNYS","4.13.2"),_=>("FX_WEEKDAYS","1")}}
fn zone(key:&QuoteKey)->chrono_tz::Tz{match market(key){"CN"=>chrono_tz::Asia::Shanghai,"HK"=>chrono_tz::Asia::Hong_Kong,"US"=>chrono_tz::America::New_York,_=>chrono_tz::Europe::London}}
fn basis(key:&QuoteKey)->&str{match market(key){"CN"|"HK"=>"unadjusted","US"=>"split_checked",_=>"daily_close"}}
fn source(key:&QuoteKey)->&str{if ["CN","HK"].contains(&market(key)){"AKShare / 新浪"}else{"yfinance / Yahoo Finance"}}
fn currency(key:&QuoteKey)->&str{match market(key){"CN"=>"CNY","HK"=>"HKD","US"=>"USD",_=>&key.code}}
fn cache_key(key:&QuoteKey,date:&str)->String{format!("{date}:{}:{}:{}",key.market,key.code,key.kind)}
fn final_day(key:&QuoteKey,date:NaiveDate,now:i64)->bool{let Some(now)=DateTime::<Utc>::from_timestamp(now,0)else{return false;};if key.kind=="fx"{return date<now.date_naive();}let local=now.with_timezone(&zone(key));date<local.date_naive()||(date==local.date_naive()&&local.hour()*60+local.minute()>=if key.market=="CN"{915}else{990})}
fn percent(index:i64)->String{let signed=(index as i128-UNIT as i128)*100;let n=(signed.abs()+500_000)/1_000_000;format!("{}{}.{:06}",if signed<0&&n!=0{"-"}else{""},n/1_000_000,n%1_000_000)}
struct Context{frozen:Ledger,ids:Vec<String>,start:NaiveDate,end:NaiveDate,base:NaiveDate,keys:Vec<QuoteKey>,needed:BTreeMap<QuoteKey,BTreeSet<NaiveDate>>,days:Vec<(NaiveDate,Vec<DatedAccount>)>,warnings:Vec<String>,valid_start:bool}
struct Cached{point:ValuationPoint,received:String,error:Option<String>}
pub(super) struct HistoryCache{sessions:BTreeMap<(String,String),(Option<String>,Option<String>)>,prices:BTreeMap<(QuoteKey,String),Cached>}
impl HistoryCache{
 pub(super) fn session(&self,key:&QuoteKey,date:&str)->Option<&(Option<String>,Option<String>)>{self.sessions.get(&(market(key).into(),date.into()))}
 fn point(&self,key:&QuoteKey,date:&str)->Option<&Cached>{self.prices.get(&(key.clone(),date.into()))}
 pub(super) fn sources(&self,keys:&[QuoteKey],date:&str,now:i64)->(Vec<ValuationSource>,Vec<String>){let mut sources=Vec::new();let mut carried=BTreeSet::new();for key in keys{
  let session=self.session(key,date);let actual=session.and_then(|s|s.0.as_deref()).unwrap_or(date);let cached=self.point(key,actual);let error=if cached.is_some(){cached.and_then(|c|c.error.clone())}else{Some(if !final_day(key,day(actual).expect("validated date"),now){"该日尚未完成收盘核对".into()}else{session.and_then(|s|s.1.clone()).unwrap_or_else(||format!("缺少 {}:{} 的 {actual} 已核对价格 / 汇率",key.market,key.code))})};
  if actual!=date{carried.insert(format!("{} 休市参考 {actual} 收盘",market(key)));}
  sources.push(ValuationSource{key:key.clone(),date:Some(actual.into()),value:cached.map(|c|c.point.value.clone()),currency:currency(key).into(),source:source(key).into(),basis:basis(key).into(),source_time:cached.and_then(|c|c.point.source_time.clone()),quoted_at:None,received_at:cached.map(|c|c.received.clone()),attempted_at:None,status:if cached.is_none(){"missing"}else if error.is_some(){"stale"}else{"daily"}.into(),error,retry_at:None});
 }(sources,carried.into_iter().collect())}
 fn needs(&self,key:&QuoteKey,dates:impl Iterator<Item=NaiveDate>,now:i64)->bool{for date in dates{let text=date.to_string();match self.session(key,&text){None=>return true,Some((None,_))=>{},Some((Some(actual),_))=>{if final_day(key,day(actual).expect("validated session"),now)&&self.point(key,actual).is_none_or(|c|c.error.is_some()){return true;}}}}false}
}
impl Ledger{
 fn performance_context(&self,q:&PerformanceQuery,now:i64)->Result<Context,String>{
  if !["CNY","USD","HKD"].contains(&q.currency.as_str()){return Err("请选择人民币、美元或港币".into());}
  let today=DateTime::<Utc>::from_timestamp(now,0).ok_or("系统时间无效")?.with_timezone(&chrono_tz::Asia::Shanghai).date_naive();let frozen=backup::copy_to_memory(&self.connection)?;let snapshot=frozen.snapshot()?;
  let ids=if q.account_ids.is_empty(){snapshot.accounts.iter().map(|a|a.id.clone()).collect::<Vec<_>>()}else{q.account_ids.clone()};if ids.iter().collect::<BTreeSet<_>>().len()!=ids.len()||ids.iter().any(|id|!snapshot.accounts.iter().any(|a|&a.id==id)){return Err("账户范围已变化，请刷新".into());}
  let withdrawals=super::imports::withdrawals(&frozen.connection)?;let mut seeds=Vec::new();let mut missing_start=false;
  for id in &ids{let opening=snapshot.openings.iter().find(|o|!o.deleted&&&o.input.account_id==id);let seed=opening.map(|o|o.input.occurred_on.as_str()).or_else(||snapshot.entries.iter().filter(|e|!e.deleted&&&e.account_id==id).map(|e|e.occurred_on.as_str()).chain(snapshot.trades.iter().filter(|t|!t.deleted&&&t.input.account_id==id).map(|t|t.input.occurred_on.as_str())).chain(snapshot.movements.iter().filter(|m|!m.deleted&&(&m.input.from_account_id==id||&m.input.to_account_id==id)).map(|m|m.input.occurred_on.as_str())).chain(snapshot.dividends.iter().filter(|d|!d.deleted&&&d.input.account_id==id).map(|d|d.input.occurred_on.as_str())).chain(withdrawals.iter().filter(|w|&w.0==id).map(|w|w.1.as_str())).min());if let Some(seed)=seed{seeds.push(day(seed)?);}else{missing_start=true;}}
  let requested_end=if q.range=="custom"{day(q.end_on.as_deref().ok_or("请选择结束日期")?)?}else{today};let requested_start=match q.range.as_str(){"month"=>today.checked_sub_months(Months::new(1)).ok_or("日期超出支持范围")?,"ytd"=>NaiveDate::from_ymd_opt(chrono::Datelike::year(&today),1,1).ok_or("日期无效")?,"all"=>seeds.iter().min().copied().unwrap_or(today),"custom"=>day(q.start_on.as_deref().ok_or("请选择开始日期")?)?,_=>return Err("请选择近月、今年以来、全部或自定义区间".into())};
  if requested_start>requested_end||requested_end>today||(requested_end-requested_start).num_days()>36600{return Err("日期区间无效、在未来或超过一百年".into());}
  let common=if !missing_start&&!ids.is_empty(){seeds.iter().max().copied()}else{None};let valid_start=common.is_some_and(|s|s<=requested_end);let start=common.filter(|s|*s<=requested_end).map(|s|s.max(requested_start)).unwrap_or(requested_start);let base=if common.is_some_and(|s|s<start){start-Duration::days(1)}else{start};let mut warnings=Vec::new();if start>requested_start{warnings.push(format!("请求从 {requested_start} 开始；共同可核对起点为 {start}，此前不回推表现"));}if !valid_start{warnings.push("所选账户尚无共同可核对起点，请补充期初或选择已建账账户".into());}
  let mut needed:BTreeMap<QuoteKey,BTreeSet<NaiveDate>>=BTreeMap::new();let mut days=Vec::new();let mut date=base;
  let account_currencies=snapshot.accounts.iter().map(|a|(&a.id,&a.currency)).collect::<BTreeMap<_,_>>();
  while date<=requested_end{let text=date.to_string();let mut accounts=frozen.snapshot_on(&text)?.accounts;accounts.retain(|a|ids.contains(&a.account_id));let mut currencies=BTreeSet::new();
   for a in &accounts{if a.balance.as_deref().is_some_and(|v|v!="0.00")||a.receivable.as_deref().is_some_and(|v|v!="0.00")||a.positions.iter().any(|p|p.quantity!="0"){currencies.insert(a.currency.clone());}for p in a.positions.iter().filter(|p|p.quantity!="0"){needed.entry(QuoteKey{market:p.market.clone(),code:p.code.clone(),kind:p.kind.clone()}).or_default().insert(date);}}
   if date>base{for e in snapshot.entries.iter().filter(|e|!e.deleted&&ids.contains(&e.account_id)&&e.occurred_on==text){if let Some(c)=account_currencies.get(&e.account_id){currencies.insert((*c).clone());}}for w in withdrawals.iter().filter(|w|ids.contains(&w.0)&&w.1==text){if let Some(c)=account_currencies.get(&w.0){currencies.insert((*c).clone());}}for m in snapshot.movements.iter().filter(|m|!m.deleted&&m.input.occurred_on==text){let from=ids.contains(&m.input.from_account_id);let to=ids.contains(&m.input.to_account_id);if from!=to{currencies.insert(if from{m.input.from_currency.clone()}else{m.input.to_currency.clone()});}}}
   if currencies.iter().any(|c|c!=&q.currency){currencies.insert(q.currency.clone());for currency in currencies.into_iter().filter(|c|c!="USD"){needed.entry(QuoteKey{market:"USD".into(),code:currency,kind:"fx".into()}).or_default().insert(date);}}
   days.push((date,accounts));date+=Duration::days(1);
  }
  let keys=needed.keys().cloned().collect::<Vec<_>>();if keys.len()>302{return Err("单次曲线最多支持 300 项证券".into());}Ok(Context{frozen,ids,start,end:requested_end,base,keys,needed,days,warnings,valid_start})
 }
 pub(super) fn performance_cache(&self,start:NaiveDate,end:NaiveDate,now:i64)->Result<HistoryCache,String>{
  let tx=self.market.connection.unchecked_transaction().map_err(database_error)?;let mut sessions=BTreeMap::new();let mut prices=BTreeMap::new();
  let mut statement=tx.prepare("SELECT market,date,session,reason,calendar,version FROM history_sessions WHERE date>=?1 AND date<=?2").map_err(database_error)?;let rows=statement.query_map(params![start.to_string(),end.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).map_err(database_error)?;
  for row in rows{let(m,d,s,reason,name,version)=row.map_err(database_error)?;let valid=day(&d).is_ok()&&calendar(&m)==(name.as_str(),version.as_str())&&s.as_ref().is_none_or(|s|day(s).is_ok_and(|v|v<=day(&d).expect("date")&&(day(&d).expect("date")-v).num_days()<=31));if valid{sessions.insert((m,d),(s,reason));}}
  drop(statement);let mut statement=tx.prepare("SELECT key,data,error FROM valuation_data WHERE substr(key,1,10)>=?1 AND substr(key,1,10)<=?2 AND data IS NOT NULL").map_err(database_error)?;let rows=statement.query_map(params![(start-Duration::days(31)).to_string(),end.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?))).map_err(database_error)?;
  for row in rows{let(id,data,error)=row.map_err(database_error)?;if let Ok(c)=serde_json::from_str::<valuation::CachedPoint>(&data){if let Some(d)=c.point.date.as_ref(){let job=ValuationJob{ticket:String::new(),date:Some(d.clone()),keys:vec![c.point.key.clone()]};if cache_key(&c.point.key,d)==id&&valuation::validate_point(&c.point,&job,now).is_ok(){prices.insert((c.point.key.clone(),d.clone()),Cached{point:c.point,received:c.received_at,error});}}}}
  drop(statement);tx.commit().map_err(database_error)?;Ok(HistoryCache{sessions,prices})
 }
 fn performance_batches(context:&Context,cache:&HistoryCache,now:i64)->Vec<(NaiveDate,NaiveDate,Vec<QuoteKey>,bool)>{
  let mut batches=Vec::new();if !context.valid_start{return batches;}let mut start=context.base;while start<=context.end{let end=(start+Duration::days(3659)).min(context.end);let active=context.keys.iter().filter(|k|context.needed.get(*k).is_some_and(|d|d.range(start..=end).next().is_some())).cloned().collect::<Vec<_>>();for keys in active.chunks(10){let needed=keys.iter().any(|k|cache.needs(k,context.needed[k].range(start..=end).copied(),now));batches.push((start,end,keys.to_vec(),needed));}start=end+Duration::days(1);}batches
 }
 pub fn performance(&self,q:&PerformanceQuery,now:i64)->Result<PerformanceState,String>{
  let c=self.performance_context(q,now)?;let cache=self.performance_cache(c.base,c.end,now)?;let batches=Self::performance_batches(&c,&cache,now);let pending=batches.iter().filter(|b|b.3).count();let mut warnings=c.warnings.clone();let mut points=Vec::new();let mut previous=None;let mut index=Some(UNIT);let mut baseline=None;let mut broken=false;let mut links=0;
  for(date,accounts)in c.days{let text=date.to_string();let active=c.keys.iter().filter(|key|c.needed.get(*key).is_some_and(|d|d.contains(&date))).cloned().collect::<Vec<_>>();let(source,carried)=cache.sources(&active,&text,now);let query=ValuationQuery{account_ids:c.ids.clone(),currency:q.currency.clone(),date:Some(text.clone())};let value=self.value_accounts(&query,now,text.clone(),accounts,source.clone())?;let mut reasons=value.missing.clone();for s in source.iter().filter(|s|s.value.is_some()&&s.error.is_some()){reasons.push(format!("{}:{} 沿用有效缓存，来源 {}，时间 {}；{}",s.key.market,s.key.code,s.source,s.source_time.as_deref().unwrap_or("未知"),s.error.as_deref().unwrap_or("刷新未完成")));}let mut incoming=Some(0i64);let mut outgoing=Some(0i64);
   if date>c.base&&c.valid_start{match c.frozen.cash_flows(&c.ids,&text,&text){Ok(flow)if flow.unavailable_reason.is_none()=>{for f in flow.totals{for(original,target)in [(&f.inflow,&mut incoming),(&f.outflow,&mut outgoing)]{match trades::scaled(original,2,true).and_then(|n|valuation::convert(n,&f.currency,&q.currency,&source)){Ok(n)=>*target=target.and_then(|v|v.checked_add(n)),Err(error)=>{*target=None;reasons.push(error);}}}}},Ok(flow)=>{incoming=None;outgoing=None;reasons.push(flow.unavailable_reason.unwrap_or_else(||"外部资金流不足".into()));},Err(error)=>{incoming=None;outgoing=None;reasons.push(error);}}}
   let amount=value.total.as_ref().map(|v|trades::scaled(v,2,true)).transpose()?;let mut daily=None;let mut cumulative=None;let status;
   if amount.is_none()||incoming.is_none()||outgoing.is_none()||!c.valid_start{status="missing";broken=true;index=None;previous=None;if !c.valid_start{reasons.push("所选账户未建立共同可核对起点".into());}}
   else if date==c.base{baseline=Some(text.clone());previous=amount;status="baseline";if amount.is_some_and(|v|v>0){cumulative=Some("0.000000".into());}else{reasons.push("零资产基准尚无参与计算的本金".into());}}
   else if let Some(prior)=previous{let denominator=prior as i128+incoming.unwrap_or(0)as i128;let numerator=amount.unwrap_or(0)as i128+outgoing.unwrap_or(0)as i128;if denominator<=0{status="inactive";reasons.push("当日没有可计算收益率的本金".into());}else{let factor=i64::try_from((numerator*UNIT as i128+denominator/2)/denominator).ok();if let Some(factor)=factor{daily=Some(percent(factor));if !broken{index=index.and_then(|i|i64::try_from((i as i128*factor as i128+UNIT as i128/2)/UNIT as i128).ok());cumulative=index.map(percent);if cumulative.is_none(){broken=true;reasons.push("累计收益率超出支持范围".into());}}status=if cumulative.is_some(){"ok"}else{"missing"};links+=1;}else{status="missing";broken=true;index=None;reasons.push("当日收益率超出支持范围".into());}}previous=amount;}
   else{status="missing";previous=amount;reasons.push("缺少前一日可核对资产；不跨缺口拼接累计收益".into());}
   reasons.extend(carried);if date>=c.start{points.push(PerformancePoint{date:text,value:value.total,inflow:incoming.map(amount_text),outflow:outgoing.map(amount_text),daily_return:daily,cumulative_return:cumulative,status:if status=="ok"&&source.iter().any(|s|s.value.is_some()&&s.error.is_some()){"stale".into()}else{status.into()},reasons});}
  }
  if links==0{warnings.push("目前只有收盘基准或没有可计算本金，尚不能给出区间收益率".into());}
  warnings.push("采用未复权价格与税后分红应收 / 实际到账；起点收盘为基准，当日基准以前的表现不回推。交易日历无法核对时不自动填补".into());if self.market.warning.is_some(){warnings.push(self.market.warning.clone().unwrap_or_default());}
  let total=if !broken&&links>0{index.map(percent)}else{None};let schedule:Option<(i64,Option<String>)>=self.market.connection.query_row("SELECT next_retry,ticket FROM market_feeds WHERE id=?1",[FEED],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(database_error)?;
  Ok(PerformanceState{start_on:c.start.to_string(),end_on:c.end.to_string(),currency:q.currency.clone(),baseline_on:baseline,total_return:total.clone(),complete:total.is_some(),method:"日频时间加权：入金按日初、出金按日末，(当日资产+外流)/(前日资产+流入)逐日链接；日内时点采用约定而非真实逐笔估值".into(),points,warnings,refresh:PerformanceRefresh{pending,retry_at:schedule.as_ref().map(|s|timestamp(s.0)).transpose()?,error:None,active:schedule.as_ref().is_some_and(|s|s.1.is_some()&&now<s.0)}})
 }

 fn money_context(&self,q:&PerformanceQuery,now:i64)->Result<(Context,Vec<(NaiveDate,super::FlowSummary)>),String>{
  let mut c=self.performance_context(q,now)?;let requested_end=c.end;let original_start=c.start;let original_warnings=c.warnings.clone();
  if q.range!="custom"{for _ in 0..3{let cache=self.performance_cache(c.end,c.end,now)?;let text=c.end.to_string();let unfinished=c.keys.iter().filter(|k|c.needed[*k].contains(&c.end)).any(|k|{let actual=cache.session(k,&text).and_then(|s|s.0.as_deref()).and_then(|s|day(s).ok()).unwrap_or(c.end);!final_day(k,actual,now)});if !unfinished{break;}let end=c.end-Duration::days(1);let before_period=end<c.start;let requested_start=original_start;let effective=PerformanceQuery{account_ids:q.account_ids.clone(),currency:q.currency.clone(),range:"custom".into(),start_on:Some(c.start.min(end).to_string()),end_on:Some(end.to_string())};c=self.performance_context(&effective,now)?;if before_period{c.start=c.end;c.base=c.end;c.days.retain(|(date,_)|*date==c.end);c.warnings.push(format!("请求从 {requested_start} 开始，该区间尚无已完成收盘日期；仅展示 {} 收盘基准，不计算区间以前的回报",c.end));}}
   if c.end<requested_end{for warning in original_warnings{if !c.warnings.contains(&warning){c.warnings.push(warning);}}c.warnings.push(format!("默认截至 {}（已完成收盘时段）；后续日期的资金流和资产不参与本次计算，自定义日期保持明确选择",c.end));}
  }
  let mut flows=Vec::new();let mut flow_days=BTreeSet::new();
  if c.valid_start{for(date,_)in &c.days{if *date>c.base{let text=date.to_string();let flow=c.frozen.cash_flows(&c.ids,&text,&text)?;if flow.unavailable_reason.is_some()||flow.totals.iter().any(|f|f.inflow!="0.00"||f.outflow!="0.00"){flow_days.insert(*date);flows.push((*date,flow));}}}}
  for(key,days)in &mut c.needed{days.retain(|d|*d==c.base||*d==c.end||key.kind=="fx"&&flow_days.contains(d));}c.keys.retain(|k|!c.needed[k].is_empty());Ok((c,flows))
 }
 pub fn money_weighted(&self,q:&PerformanceQuery,now:i64)->Result<super::MoneyWeightedState,String>{
  use super::{MoneyWeightedFlow,MoneyWeightedState};let(c,external)=self.money_context(q,now)?;let cache=self.performance_cache(c.base,c.end,now)?;let pending=Self::performance_batches(&c,&cache,now).iter().filter(|b|b.3).count();let mut opening=None;let mut closing=None;let mut warnings=c.warnings.clone();
  for(date,accounts)in c.days{if date!=c.base&&date!=c.end{continue;}let text=date.to_string();let active=c.keys.iter().filter(|k|c.needed[*k].contains(&date)).cloned().collect::<Vec<_>>();let(sources,mut reasons)=cache.sources(&active,&text,now);let value=self.value_accounts(&ValuationQuery{account_ids:c.ids.clone(),currency:q.currency.clone(),date:Some(text.clone())},now,text.clone(),accounts,sources.clone())?;reasons.extend(value.missing);for s in sources.iter().filter(|s|s.error.is_some()&&s.value.is_some()){reasons.push(format!("{}:{} 沿用缓存，来源时间 {}；{}",s.key.market,s.key.code,s.source_time.as_deref().unwrap_or("未知"),s.error.as_deref().unwrap_or("")));}if !c.valid_start{reasons.push("所选账户没有共同可核对起点".into());}
   let amount=if c.valid_start{value.total}else{None};if date==c.base{opening=Some(MoneyWeightedFlow{date:text.clone(),kind:"opening".into(),amount:amount.as_deref().map(|v|trades::scaled(v,2,true).and_then(|v|v.checked_neg().ok_or_else(||"期初金额溢出".into())).map(amount_text)).transpose()?,reasons:reasons.clone()});}if date==c.end{closing=Some(MoneyWeightedFlow{date:text,kind:"closing".into(),amount,reasons});}
  }
  let mut flows=vec![opening.ok_or("缺少起点")?];
  for(date,flow)in external{let text=date.to_string();let active=c.keys.iter().filter(|k|k.kind=="fx"&&c.needed[*k].contains(&date)).cloned().collect::<Vec<_>>();let(sources,carried)=cache.sources(&active,&text,now);if let Some(reason)=flow.unavailable_reason{flows.push(MoneyWeightedFlow{date:text,kind:"unavailable".into(),amount:None,reasons:vec![reason]});continue;}
   for total in flow.totals{for(kind,original,negative)in [("contribution",total.inflow,true),("withdrawal",total.outflow,false)]{if original=="0.00"{continue;}let mut reasons=carried.clone();for s in sources.iter().filter(|s|s.error.is_some()&&s.value.is_some()){reasons.push(format!("{}:{} 沿用缓存，来源时间 {}；{}",s.key.market,s.key.code,s.source_time.as_deref().unwrap_or("未知"),s.error.as_deref().unwrap_or("")));}let amount=match trades::scaled(&original,2,true).and_then(|v|valuation::convert(v,&total.currency,&q.currency,&sources)){Ok(v)=>Some(amount_text(if negative{-v}else{v})),Err(error)=>{reasons.push(error);None}};flows.push(MoneyWeightedFlow{date:text.clone(),kind:kind.into(),amount,reasons});}}
  }
  flows.push(closing.ok_or("缺少期末")?);if let Some(w)=self.market.warning.as_ref(){warnings.push(w.clone());}warnings.push("按实际日期计算 XIRR，年化采用实际天数 / 365；区间回报由同一年化解换算。投入为负、撤回和期末资产为正，内部买卖及分红不重复作为外部资金流".into());let schedule:Option<i64>=self.market.connection.query_row("SELECT next_retry FROM market_feeds WHERE id=?1",[FEED],|r|r.get(0)).optional().map_err(database_error)?;
  super::money_weighted::calculate(MoneyWeightedState{start_on:c.start.to_string(),end_on:c.end.to_string(),baseline_on:if c.valid_start{Some(c.base.to_string())}else{None},currency:q.currency.clone(),elapsed_days:(c.end-c.base).num_days()as u32,annualized_return:None,period_return:None,status:"missing".into(),reason:None,flows,warnings,refresh:PerformanceRefresh{pending,retry_at:schedule.map(timestamp).transpose()?,error:None,active:false}})
 }
 pub fn begin_money_weighted_refresh(&mut self,q:&PerformanceQuery,now:i64)->Result<Option<PerformanceJob>,String>{let(c,_)=self.money_context(q,now)?;self.begin_history_refresh(c,now)}
 pub fn begin_performance_refresh(&mut self,q:&PerformanceQuery,now:i64)->Result<Option<PerformanceJob>,String>{
  let c=self.performance_context(q,now)?;self.begin_history_refresh(c,now)
 }
 fn begin_history_refresh(&mut self,c:Context,now:i64)->Result<Option<PerformanceJob>,String>{
  if !c.valid_start{return Err("所选账户没有共同可核对起点，请补充期初或调整范围".into());}let cache=self.performance_cache(c.base,c.end,now)?;let batches=Self::performance_batches(&c,&cache,now);let missing=batches.iter().any(|b|b.3);let mut ranked=Vec::new();
  for batch in batches.iter().filter(|b|!missing||b.3){let id=serde_json::to_string(&(batch.0.to_string(),batch.1.to_string(),&batch.2)).map_err(|_|"无法标识历史批次")?;let attempt:Option<i64>=self.market.connection.query_row("SELECT last_attempt FROM history_batches WHERE id=?1",[&id],|r|r.get(0)).optional().map_err(database_error)?;ranked.push((attempt.unwrap_or(0),id,batch));}
  ranked.sort_by_key(|b|b.0);let Some((_,_,(start,end,keys,_)))=ranked.first()else{return Ok(None);};
  self.begin_history_keys(*start,*end,keys.clone(),now)
 }
 pub(super) fn begin_history_keys(&mut self,start:NaiveDate,end:NaiveDate,keys:Vec<QuoteKey>,now:i64)->Result<Option<PerformanceJob>,String>{
  if keys.is_empty(){return Ok(None);}
  let batch_id=serde_json::to_string(&(start.to_string(),end.to_string(),&keys)).map_err(|_|"无法标识历史批次")?;
  let tx=self.market.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;let due:Option<i64>=tx.query_row("SELECT next_retry FROM market_feeds WHERE id=?1",[FEED],|r|r.get(0)).optional().map_err(database_error)?;if due.is_some_and(|d|now<d){return Ok(None);}
  tx.execute("INSERT INTO history_batches(id,last_attempt) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET last_attempt=excluded.last_attempt",params![batch_id,now]).map_err(database_error)?;
  let job=PerformanceJob{ticket:Uuid::new_v4().to_string(),start_on:start.to_string(),end_on:end.to_string(),keys:keys.clone()};let request=serde_json::to_string(&job).map_err(|_|"无法开始历史请求")?;tx.execute("INSERT INTO market_feeds(id,failures,next_retry,ticket,request,last_attempt) VALUES(?1,0,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET next_retry=excluded.next_retry,ticket=excluded.ticket,request=excluded.request,last_attempt=excluded.last_attempt",params![FEED,now+120,job.ticket,request,now]).map_err(database_error)?;tx.commit().map_err(database_error)?;Ok(Some(job))
 }
 pub fn finish_performance_refresh(&mut self,job:&PerformanceJob,response:&HistoryResponse,now:i64)->Result<(),String>{
  let received=timestamp(now)?;let start=day(&job.start_on)?;let end=day(&job.end_on)?;let tx=self.market.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;let stored:Option<(Option<String>,Option<String>,i64)>=tx.query_row("SELECT ticket,request,failures FROM market_feeds WHERE id=?1",[FEED],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(database_error)?;
  let Some((Some(ticket),Some(request),failures))=stored else{return Err("历史请求已结束".into());};if ticket!=job.ticket||serde_json::from_str::<PerformanceJob>(&request).map_err(|_|"历史请求损坏")?!=*job{return Err("历史请求已过期，请读取缓存".into());}
  let mut protocol=response.error.clone();let mut series=BTreeMap::new();let mut calendars=BTreeMap::new();let markets=job.keys.iter().map(|k|market(k).to_string()).collect::<BTreeSet<_>>();
  if response.feed!="performance"||response.series.len()>job.keys.len()||response.calendars.len()>markets.len()||response.error.is_some()&&(!response.series.is_empty()||!response.calendars.is_empty()){protocol=Some("历史响应身份或协议不符".into());}
  for s in &response.series{if !job.keys.contains(&s.key)||series.insert(s.key.clone(),s).is_some()||s.source!=source(&s.key)||s.basis!=basis(&s.key)||s.timezone!=zone(&s.key).to_string()||s.prices.len()>3700||s.invalidates_before.as_ref().is_some_and(|d|s.key.market!="US"||day(d).is_err()||day(d).is_ok_and(|d|d>DateTime::<Utc>::from_timestamp(now,0).expect("time").date_naive()+Duration::days(1))){protocol=Some("历史响应含重复、未请求证券或调整口径异常".into());}}
  for cal in &response.calendars{if !markets.contains(&cal.market)||calendar(&cal.market)!=(cal.name.as_str(),cal.version.as_str())||cal.sessions.len()!=(end-start).num_days()as usize+1||calendars.insert(cal.market.clone(),cal).is_some(){protocol=Some("历史交易日历不符".into());}let mut seen=BTreeSet::new();for row in &cal.sessions{if day(&row.date).is_err()||row.date<job.start_on||row.date>job.end_on||!seen.insert(&row.date)||row.session.as_ref().is_some_and(|s|day(s).is_err()||s>&row.date||day(s).is_ok_and(|s|(day(&row.date).unwrap_or(start)-s).num_days()>31))||row.session.is_none()&&row.reason.is_none(){protocol=Some("历史交易日历日期不符".into());}}}
  let mut valid=0;
  if protocol.is_none(){for cal in &response.calendars{for row in &cal.sessions{tx.execute("INSERT INTO history_sessions(market,date,session,reason,calendar,version) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(market,date) DO UPDATE SET session=excluded.session,reason=excluded.reason,calendar=excluded.calendar,version=excluded.version",params![cal.market,row.date,row.session,row.reason,cal.name,cal.version]).map_err(database_error)?;}}}
  for key in &job.keys{let entry=series.get(key);let mut error=protocol.clone().or_else(||entry.and_then(|s|s.error.clone()));let mut points=BTreeMap::new();if protocol.is_none(){if let Some(s)=entry{for p in &s.prices{let point=ValuationPoint{key:key.clone(),date:Some(p.date.clone()),value:p.close.clone(),currency:currency(key).into(),source:s.source.clone(),basis:s.basis.clone(),source_time:Some(format!("{} [{}]",p.date,s.timezone)),quoted_at:None};let request=ValuationJob{ticket:String::new(),date:point.date.clone(),keys:vec![key.clone()]};if day(&p.date).is_err()||day(&p.date).is_ok_and(|d|d<start-Duration::days(31)||d>end)||valuation::validate_point(&point,&request,now).is_err()||points.insert(p.date.clone(),point).is_some(){error=Some("历史序列含非法价格或日期，沿用有效缓存".into());points.clear();break;}}
   if let Some(cutoff)=s.invalidates_before.as_ref(){let pattern=format!("%:{}:{}:{}",key.market,key.code,key.kind);tx.execute("UPDATE valuation_data SET data=NULL,error='历史拆股口径校验失效，停用旧价格缓存',last_attempt=?3 WHERE key LIKE ?1 AND substr(key,1,10)<?2",params![pattern,cutoff,now]).map_err(database_error)?;}}
  }
  for(date,point)in &points{let cached=valuation::CachedPoint{point:point.clone(),received_at:received.clone()};tx.execute("INSERT INTO valuation_data(key,data,error,last_attempt) VALUES(?1,?2,NULL,?3) ON CONFLICT(key) DO UPDATE SET data=excluded.data,error=NULL,last_attempt=excluded.last_attempt",params![cache_key(key,date),serde_json::to_string(&cached).map_err(|_|"无法保存历史价格")?,now]).map_err(database_error)?;valid+=1;}
  let cal=calendars.get(market(key));let needed=if let Some(cal)=cal{cal.sessions.iter().filter_map(|s|s.session.clone()).collect::<BTreeSet<_>>()}else{let mut dates=BTreeSet::new();let mut d=start;while d<=end{dates.insert(d.to_string());d+=Duration::days(1);}dates};
  for actual in needed{if !points.contains_key(&actual)&&final_day(key,day(&actual)?,now){let message=error.clone().unwrap_or_else(||"来源缺少该交易日已核对收盘价".into());tx.execute("INSERT INTO valuation_data(key,data,error,last_attempt) VALUES(?1,NULL,?2,?3) ON CONFLICT(key) DO UPDATE SET error=excluded.error,last_attempt=excluded.last_attempt",params![cache_key(key,&actual),message.chars().take(300).collect::<String>(),now]).map_err(database_error)?;}}
 }
 let failures=if valid>0{0}else{(failures+1).min(5)};let delay=if failures==0{60}else{(60i64*2i64.pow((failures-1)as u32)).min(900)};tx.execute("UPDATE market_feeds SET failures=?1,next_retry=?2,ticket=NULL,request=NULL,last_attempt=?3 WHERE id=?4",params![failures,now+delay,now,FEED]).map_err(database_error)?;tx.commit().map_err(database_error)?;Ok(())
 }
}
