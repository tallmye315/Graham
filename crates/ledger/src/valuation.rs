use super::{amount_text,database_error,trades,DatedAccount,Ledger,QuoteKey};
use chrono::{DateTime,NaiveDate,Timelike,Utc};
use rusqlite::{params,OptionalExtension,TransactionBehavior};
use std::collections::{BTreeMap,BTreeSet};
use uuid::Uuid;
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ValuationQuery{pub account_ids:Vec<String>,pub currency:String,pub date:Option<String>}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ValuationJob{pub ticket:String,pub date:Option<String>,pub keys:Vec<QuoteKey>}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ValuationPoint{pub key:QuoteKey,pub date:Option<String>,pub value:String,pub currency:String,pub source:String,pub basis:String,pub source_time:Option<String>,pub quoted_at:Option<String>}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ValuationError{pub key:QuoteKey,pub error:String,#[serde(default)]pub invalidates:bool}
#[derive(Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ValuationResponse{pub feed:String,pub points:Vec<ValuationPoint>,pub errors:Vec<ValuationError>,pub error:Option<String>}
#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ValuationSource{pub key:QuoteKey,pub date:Option<String>,pub value:Option<String>,pub currency:String,pub source:String,pub basis:String,pub source_time:Option<String>,pub quoted_at:Option<String>,pub received_at:Option<String>,pub attempted_at:Option<String>,pub status:String,pub error:Option<String>,pub retry_at:Option<String>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ValuedAsset{pub key:Option<QuoteKey>,pub account_id:String,pub name:String,pub kind:String,pub currency:String,pub quantity:Option<String>,pub price:Option<String>,pub original_value:Option<String>,pub converted_value:Option<String>,pub source:String,pub source_time:Option<String>,pub reason:Option<String>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct CurrencyDistribution{pub currency:String,pub cash:Option<String>,pub securities:Option<String>,pub receivables:Option<String>,pub known_original:Option<String>,pub converted_value:Option<String>,pub share:Option<String>,pub complete:bool}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ValuationState{pub currency:String,pub date:String,pub historical:bool,pub complete:bool,pub total:Option<String>,pub known_total:Option<String>,pub missing:Vec<String>,pub warnings:Vec<String>,pub assets:Vec<ValuedAsset>,pub distribution:Vec<CurrencyDistribution>,pub sources:Vec<ValuationSource>,pub cache_warning:Option<String>}

#[derive(Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(super) struct CachedPoint{pub(super) point:ValuationPoint,pub(super) received_at:String}
fn time(now:i64)->Result<DateTime<Utc>,String>{DateTime::<Utc>::from_timestamp(now,0).ok_or_else(||"系统时间无效".into())}
fn cache_key(key:&QuoteKey,date:Option<&str>)->String{format!("{}:{}:{}:{}",date.unwrap_or("latest"),key.market,key.code,key.kind)}
fn request_id(date:Option<&str>)->&'static str{if date.is_some(){"valuation:daily"}else{"valuation:latest"}}
fn fx(currency:&str)->QuoteKey{QuoteKey{market:"USD".into(),code:currency.into(),kind:"fx".into()}}
fn point_currency(key:&QuoteKey)->&str{if key.kind=="fx"{&key.code}else{match key.market.as_str(){"CN"=>"CNY","HK"=>"HKD",_=>"USD"}}}
fn source(key:&QuoteKey)->&str{if key.kind=="fx"||key.market=="US"{"yfinance / Yahoo Finance"}else{"AKShare / 新浪"}}
fn timezone(key:&QuoteKey)->chrono_tz::Tz{if key.kind=="fx"{chrono_tz::Europe::London}else{match key.market.as_str(){"CN"=>chrono_tz::Asia::Shanghai,"HK"=>chrono_tz::Asia::Hong_Kong,_=>chrono_tz::America::New_York}}}
pub(super) fn validate_point(point:&ValuationPoint,job:&ValuationJob,now:i64)->Result<(),String>{
 if point.date!=job.date||point.currency!=point_currency(&point.key)||point.source!=source(&point.key){return Err("来源日期、币种或身份不符".into());}
 trades::scaled(&point.value,if point.key.kind=="fx"{8}else{6},false)?;
 let now=time(now)?;
 if let Some(day)=point.date.as_deref(){
  let date=NaiveDate::parse_from_str(day,"%Y-%m-%d").map_err(|_|"历史日期无效")?;let local=now.with_timezone(&timezone(&point.key));
  let final_day=if point.key.kind=="fx"{date<now.date_naive()}else{date<local.date_naive()||(date==local.date_naive()&&local.hour()*60+local.minute()>=if point.key.market=="CN"{915}else{990})};
  if !final_day{return Err("该日日线尚未完成收盘核对".into());}
  let basis=if point.key.kind=="fx"{"daily_close"}else if point.key.market=="US"{"split_checked"}else{"unadjusted"};
  let raw=format!("{day} [{}]",timezone(&point.key));if point.basis!=basis||point.source_time.as_deref()!=Some(raw.as_str())||point.quoted_at.is_some(){return Err("历史价格调整口径或时区不符".into());}
 }else{
  if point.key.kind!="fx"||point.basis!="reference_quote"{return Err("最新估值仅接受参考汇率".into());}
  let raw=point.source_time.as_deref().ok_or("缺少汇率原始时间")?;let quoted=point.quoted_at.as_deref().ok_or("缺少汇率报价时间")?;
  let quoted=DateTime::parse_from_rfc3339(quoted).map_err(|_|"汇率时间无效")?;let raw=DateTime::parse_from_rfc3339(raw).map_err(|_|"汇率来源时间无效")?;
  if quoted!=raw||quoted.timestamp()>now.timestamp()+60{return Err("汇率时点不符或在未来".into());}
 }Ok(())
}
impl Ledger{
 pub(super) fn valuation_accounts(&self,query:&ValuationQuery,now:i64)->Result<(String,Vec<DatedAccount>),String>{
  if !["CNY","USD","HKD"].contains(&query.currency.as_str()){return Err("请选择人民币、美元或港币".into());}
  let today=time(now)?.with_timezone(&chrono_tz::Asia::Shanghai).date_naive().to_string();let date=query.date.as_deref().unwrap_or(&today);
  let parsed=NaiveDate::parse_from_str(date,"%Y-%m-%d").map_err(|_|"估值日期无效")?;if parsed.to_string()!=date||date>today.as_str(){return Err("估值日期无效或在未来".into());}
  let mut accounts=self.snapshot_on(date)?.accounts;let chosen=query.account_ids.iter().collect::<BTreeSet<_>>();if chosen.len()!=query.account_ids.len()||chosen.iter().any(|id|!accounts.iter().any(|a|a.account_id==id.as_str())){return Err("账户范围已变化，请刷新".into());}
  accounts.retain(|a|chosen.is_empty()||chosen.contains(&a.account_id));
  if query.date.is_none(){for a in &mut accounts{if !a.has_activity{a.balance=Some("0.00".into());a.receivable=Some("0.00".into());a.unavailable_reason=None;}}}
  Ok((date.into(),accounts))
 }
 pub(super) fn valuation_keys(&self,query:&ValuationQuery,now:i64,probe:bool)->Result<Vec<QuoteKey>,String>{
  let(_,accounts)=self.valuation_accounts(query,now)?;let mut keys=BTreeSet::new();let mut currencies=BTreeSet::new();
  for a in accounts{if a.balance.as_deref().is_some_and(|v|v!="0.00")||a.receivable.as_deref().is_some_and(|v|v!="0.00")||a.positions.iter().any(|p|p.quantity!="0"){currencies.insert(a.currency);}
   if query.date.is_some(){for p in a.positions.iter().filter(|p|p.quantity!="0"){keys.insert(QuoteKey{market:p.market.clone(),code:p.code.clone(),kind:p.kind.clone()});}}
  }
  if currencies.iter().any(|c|c!=&query.currency){currencies.insert(query.currency.clone());for c in currencies.iter().filter(|c|c.as_str()!="USD"){keys.insert(fx(c));}}
  if query.date.is_none()&&!currencies.is_empty(){keys.insert(fx("CNY"));keys.insert(fx("HKD"));}
  if probe{keys.insert(fx("CNY"));keys.insert(fx("HKD"));if query.date.is_some(){for(m,c,k)in [("CN","600036","stock"),("CN","510300","etf"),("HK","00700","stock"),("US","AAPL","stock"),("US","SPY","etf")]{keys.insert(QuoteKey{market:m.into(),code:c.into(),kind:k.into()});}}}
  if keys.len()>302{return Err("单次估值最多支持 300 项证券".into());}Ok(keys.into_iter().collect())
 }
 pub fn begin_valuation_refresh(&mut self,query:&ValuationQuery,now:i64,probe:bool)->Result<Option<ValuationJob>,String>{
  let keys=self.valuation_keys(query,now,probe)?;self.begin_valuation_keys(query.date.clone(),keys,now)
 }
 pub(super) fn begin_valuation_keys(&mut self,date:Option<String>,keys:Vec<QuoteKey>,now:i64)->Result<Option<ValuationJob>,String>{
  if keys.is_empty(){return Ok(None);}
  let tx=self.market.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
  let due:Option<i64>=tx.query_row("SELECT next_retry FROM market_feeds WHERE id=?1",[request_id(date.as_deref())],|r|r.get(0)).optional().map_err(database_error)?;if due.is_some_and(|due|now<due){return Ok(None);}
  let mut ages=Vec::new();for key in keys{let attempt:Option<i64>=tx.query_row("SELECT last_attempt FROM valuation_data WHERE key=?1",[cache_key(&key,date.as_deref())],|r|r.get(0)).optional().map_err(database_error)?;ages.push((attempt.unwrap_or(0),key));}ages.sort();
  let job=ValuationJob{ticket:Uuid::new_v4().to_string(),date:date.clone(),keys:ages.into_iter().take(10).map(|(_,key)|key).collect()};let json=serde_json::to_string(&job).map_err(|_|"无法开始估值请求")?;
  tx.execute("INSERT INTO market_feeds(id,failures,next_retry,ticket,request,last_attempt) VALUES(?1,0,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET next_retry=excluded.next_retry,ticket=excluded.ticket,request=excluded.request,last_attempt=excluded.last_attempt",params![request_id(job.date.as_deref()),now+120,job.ticket,json,now]).map_err(database_error)?;
  for key in &job.keys{tx.execute("INSERT INTO valuation_data(key,data,error,last_attempt) VALUES(?1,NULL,NULL,?2) ON CONFLICT(key) DO UPDATE SET last_attempt=excluded.last_attempt",params![cache_key(key,job.date.as_deref()),now]).map_err(database_error)?;}tx.commit().map_err(database_error)?;Ok(Some(job))
 }
 pub fn finish_valuation_refresh(&mut self,job:&ValuationJob,response:&ValuationResponse,now:i64)->Result<(),String>{
  let received=time(now)?.to_rfc3339();let tx=self.market.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
  let saved:Option<(Option<String>,Option<String>,i64)>=tx.query_row("SELECT ticket,request,failures FROM market_feeds WHERE id=?1",[request_id(job.date.as_deref())],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(database_error)?;
  let Some((Some(ticket),Some(request),failures))=saved else{return Err("估值请求已结束".into());};if ticket!=job.ticket||serde_json::from_str::<ValuationJob>(&request).map_err(|_|"估值请求损坏")?!=*job{return Err("估值请求已过期，请刷新".into());}
  let mut protocol=response.error.clone();let mut points=BTreeMap::new();let mut errors=BTreeMap::new();
  if response.feed!="valuation"||response.points.len()+response.errors.len()>job.keys.len()||response.error.is_some()&&(!response.points.is_empty()||!response.errors.is_empty()){protocol=Some("估值响应协议不符".into());}
  for p in &response.points{if !job.keys.contains(&p.key)||points.insert(p.key.clone(),p).is_some(){protocol=Some("估值响应含未请求或重复价格".into());}}
  for e in &response.errors{if !job.keys.contains(&e.key)||points.contains_key(&e.key)||errors.insert(e.key.clone(),e).is_some(){protocol=Some("估值响应含重复或未请求缺项".into());}}
  let mut valid=0;for key in &job.keys{let id=cache_key(key,job.date.as_deref());let old:Option<String>=tx.query_row("SELECT data FROM valuation_data WHERE key=?1",[&id],|r|r.get(0)).optional().map_err(database_error)?.flatten();let old=old.and_then(|s|serde_json::from_str::<CachedPoint>(&s).ok());
   let result=if let Some(error)=&protocol{Err(error.clone())}else if let Some(point)=points.get(key){validate_point(point,job,now).and_then(|_|{if let(Some(previous),Some(incoming))=(old.as_ref().and_then(|p|p.point.quoted_at.as_ref()).and_then(|s|DateTime::parse_from_rfc3339(s).ok()),point.quoted_at.as_ref().and_then(|s|DateTime::parse_from_rfc3339(s).ok())){if incoming<previous{return Err("来源返回更早汇率，沿用有效旧值".into());}}Ok(CachedPoint{point:(*point).clone(),received_at:received.clone()})})}else{Err(errors.get(key).map(|e|e.error.clone()).unwrap_or_else(||"来源缺少该日期价格或汇率；不以其他日期补齐".into()))};
   match result{Ok(point)=>{let data=serde_json::to_string(&point).map_err(|_|"无法保存估值缓存")?;tx.execute("UPDATE valuation_data SET data=?2,error=NULL,last_attempt=?3 WHERE key=?1",params![id,data,now]).map_err(database_error)?;valid+=1;},Err(error)=>{tx.execute("UPDATE valuation_data SET error=?2,last_attempt=?3,data=CASE WHEN ?4 THEN NULL ELSE data END WHERE key=?1",params![id,error.chars().take(300).collect::<String>(),now,protocol.is_none()&&errors.get(key).is_some_and(|e|e.invalidates)]).map_err(database_error)?;}}
  }
  let failures=if valid>0{0}else{(failures+1).min(5)};let minimum=if job.date.is_none(){300i64}else{60};let delay=if failures==0{minimum}else{(minimum*2i64.pow((failures-1)as u32)).min(900)};
  tx.execute("UPDATE market_feeds SET failures=?1,next_retry=?2,ticket=NULL,request=NULL,last_attempt=?3 WHERE id=?4",params![failures,now+delay,now,request_id(job.date.as_deref())]).map_err(database_error)?;tx.commit().map_err(database_error)?;Ok(())
 }
 pub(super) fn valuation_sources(&self,keys:Vec<QuoteKey>,date:Option<&str>,now:i64)->Result<Vec<ValuationSource>,String>{
  let tx=self.market.connection.unchecked_transaction().map_err(database_error)?;let mut result=Vec::new();let retry:Option<i64>=tx.query_row("SELECT next_retry FROM market_feeds WHERE id=?1",[request_id(date)],|r|r.get(0)).optional().map_err(database_error)?;
  for key in keys{let row:Option<(Option<String>,Option<String>,i64)>=tx.query_row("SELECT data,error,last_attempt FROM valuation_data WHERE key=?1",[cache_key(&key,date)],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(database_error)?;let mut error=row.as_ref().and_then(|r|r.1.clone());let data=row.as_ref().and_then(|r|r.0.as_ref()).and_then(|s|serde_json::from_str::<CachedPoint>(s).ok()).filter(|p|p.point.key==key&&p.point.date.as_deref()==date&&p.point.currency==point_currency(&key)&&trades::scaled(&p.point.value,if key.kind=="fx"{8}else{6},false).is_ok());
   let status=if data.is_none(){if error.is_none(){error=Some("缺少有效缓存，请刷新来源".into());}"missing"}else if error.is_some()||date.is_none()&&data.as_ref().and_then(|p|p.point.quoted_at.as_ref()).and_then(|s|DateTime::parse_from_rfc3339(s).ok()).is_some_and(|d|now-d.timestamp()>900){"stale"}else{if date.is_some(){"daily"}else{"reference"}};
   result.push(ValuationSource{currency:point_currency(&key).into(),source:data.as_ref().map(|p|p.point.source.clone()).unwrap_or_else(||source(&key).into()),basis:data.as_ref().map(|p|p.point.basis.clone()).unwrap_or_default(),value:data.as_ref().map(|p|p.point.value.clone()),source_time:data.as_ref().and_then(|p|p.point.source_time.clone()),quoted_at:data.as_ref().and_then(|p|p.point.quoted_at.clone()),received_at:data.as_ref().map(|p|p.received_at.clone()),attempted_at:row.as_ref().map(|r|time(r.2).map(|d|d.to_rfc3339())).transpose()?,status:status.into(),error,retry_at:retry.map(|r|time(r).map(|d|d.to_rfc3339())).transpose()?,key,date:date.map(str::to_string)});
  }tx.commit().map_err(database_error)?;Ok(result)
 }
 pub fn valuation(&self,query:&ValuationQuery,now:i64,probe:bool)->Result<ValuationState,String>{
  let(date,accounts)=self.valuation_accounts(query,now)?;let sources=self.valuation_sources(self.valuation_keys(query,now,probe)?,query.date.as_deref(),now)?;self.value_accounts(query,now,date,accounts,sources)
 }
 pub(super) fn value_accounts(&self,query:&ValuationQuery,now:i64,date:String,accounts:Vec<DatedAccount>,sources:Vec<ValuationSource>)->Result<ValuationState,String>{
  let mut assets=Vec::new();let mut warnings=BTreeSet::new();let mut missing=BTreeSet::new();
  let keys=accounts.iter().flat_map(|a|a.positions.iter().filter(|p|p.quantity!="0").map(|p|QuoteKey{market:p.market.clone(),code:p.code.clone(),kind:p.kind.clone()})).collect::<BTreeSet<_>>();let quotes=if query.date.is_none(){self.market.views(keys.into_iter().collect(),now)?}else{Vec::new()};
  for account in accounts{
   assets.push(ValuedAsset{key:None,account_id:account.account_id.clone(),name:format!("{} · 现金",account.name),kind:"cash".into(),currency:account.currency.clone(),quantity:None,price:None,original_value:account.balance,converted_value:None,source:"账户记录".into(),source_time:Some(date.clone()),reason:account.unavailable_reason.clone()});
   if account.receivable.as_deref()!=Some("0.00"){assets.push(ValuedAsset{key:None,account_id:account.account_id.clone(),name:format!("{} · 待到账分红",account.name),kind:"receivable".into(),currency:account.currency.clone(),quantity:None,price:None,original_value:account.receivable,converted_value:None,source:"分红确认 / 实际到账记录".into(),source_time:Some(date.clone()),reason:account.unavailable_reason.clone().or_else(||Some("待到账分红金额超出支持范围".into()))});}
   for position in account.positions.iter().filter(|p|p.quantity!="0"){
    let key=QuoteKey{market:position.market.clone(),code:position.code.clone(),kind:position.kind.clone()};let(price,source,time,error)=if query.date.is_some(){let point=sources.iter().find(|s|s.key==key);(point.and_then(|s|s.value.clone()),point.map(|s|s.source.clone()).unwrap_or_default(),point.and_then(|s|s.source_time.clone()),point.and_then(|s|s.error.clone()))}else{let q=quotes.iter().find(|q|q.key==key);if let Some(q)=q{if q.status=="stale"||q.status=="time_unknown"{warnings.insert(format!("{}:{} 沿用旧报价或来源日期不完整；请核对时点",key.market,key.code));}}(q.and_then(|q|q.price.clone()),q.map(|q|q.source.clone()).unwrap_or_default(),q.and_then(|q|q.quoted_at.clone().or_else(||q.source_time.clone())),q.and_then(|q|q.error.clone()))};
    let value=price.as_ref().map(|price|trades::value_at_position(position,price));let(original,reason)=match value{Some(Ok(v))=>(Some(v.market_value),None),Some(Err(e))=>(None,Some(e)),None=>(None,Some(error.unwrap_or_else(||format!("缺少 {} 对应价格",date))))};
    assets.push(ValuedAsset{key:Some(key),account_id:account.account_id.clone(),name:format!("{} · {}:{} {}",account.name,position.market,position.code,position.name),kind:"security".into(),currency:position.currency.clone(),quantity:Some(position.quantity.clone()),price,original_value:original,converted_value:None,source,source_time:time,reason});
   }
  }
  for asset in &mut assets{
   if let Some(value)=asset.original_value.as_ref(){let converted=trades::scaled(value,2,true).and_then(|amount|convert(amount,&asset.currency,&query.currency,&sources));match converted{Ok(value)=>{asset.converted_value=Some(amount_text(value));if asset.kind=="receivable"{asset.reason=None;}},Err(error)=>asset.reason=Some(error)}}
   if asset.converted_value.is_none(){missing.insert(format!("{}：{}",asset.name,asset.reason.as_deref().unwrap_or("该日数据不足")));}
  }
  for source in &sources{if source.status=="stale"{warnings.insert(format!("{}:{} 沿用缓存值，来源时间 {}",source.key.market,source.key.code,source.source_time.as_deref().unwrap_or("未知")));}}
  let sum=|values:Vec<i64>|values.into_iter().try_fold(0i64,|sum,value|sum.checked_add(value));let converted=assets.iter().filter_map(|a|a.converted_value.as_ref()).map(|v|trades::scaled(v,2,true)).collect::<Result<Vec<_>,_>>()?;let known=sum(converted);if known.is_none(){missing.insert("汇总金额超出支持范围，请缩小账户范围".into());}
  let mut distribution=Vec::new();for currency in ["CNY","USD","HKD"]{let group=assets.iter().filter(|a|a.currency==currency).collect::<Vec<_>>();let subtotal=|kind:&str|->Option<String>{if group.iter().any(|a|a.kind==kind&&a.original_value.is_none()){return None;}let values=group.iter().filter(|a|a.kind==kind).filter_map(|a|a.original_value.as_ref()).map(|v|trades::scaled(v,2,true)).collect::<Result<Vec<_>,_>>().ok()?;sum(values).map(amount_text)};
   let original=group.iter().filter_map(|a|a.original_value.as_ref()).map(|v|trades::scaled(v,2,true)).collect::<Result<Vec<_>,_>>()?;let known_original=sum(original).map(amount_text);let complete=group.iter().all(|a|a.converted_value.is_some())&&known_original.is_some();let value=if complete{sum(group.iter().filter_map(|a|a.converted_value.as_ref()).map(|v|trades::scaled(v,2,true)).collect::<Result<Vec<_>,_>>()?)}else{None};if known_original.is_none()||complete&&value.is_none(){missing.insert(format!("{currency} 币种分布金额超出支持范围"));}
   distribution.push(CurrencyDistribution{currency:currency.into(),cash:subtotal("cash"),securities:subtotal("security"),receivables:subtotal("receivable"),known_original,converted_value:value.map(amount_text),share:None,complete:complete&&value.is_some()});
  }
  let complete=missing.is_empty();if complete{if let Some(total)=known.filter(|v|*v>0){for d in &mut distribution{if let Some(value)=d.converted_value.as_ref(){let n=trades::scaled(value,2,true)? as i128*10000;let percent=(n+total as i128/2)/total as i128;d.share=Some(format!("{}.{:02}",percent/100,percent%100));}}}}
  if query.date.is_none(){warnings.insert("当前参考估值使用最后有效报价与参考汇率，可能含延时、旧收盘价或不同时点；不代表实时可成交净值".into());}else{warnings.insert("历史价格与汇率精确匹配所选日期，休市或来源缺项不以其他日期补齐；日线按各市场时区，仅核对已过收盘缓冲的日期".into());}
  Ok(ValuationState{currency:query.currency.clone(),date,historical:query.date.is_some(),complete,total:if complete{known.map(amount_text)}else{None},known_total:known.map(amount_text),missing:missing.into_iter().collect(),warnings:warnings.into_iter().collect(),assets,distribution,sources,cache_warning:self.market.warning.clone()})
 }
}
pub(super) fn convert(amount:i64,from:&str,to:&str,sources:&[ValuationSource])->Result<i64,String>{
 if amount==0||from==to{return Ok(amount);}
 let rate=|currency:&str|->Result<i64,String>{if currency=="USD"{return Ok(100_000_000);}sources.iter().find(|s|s.key==fx(currency)).and_then(|s|s.value.as_ref()).ok_or_else(||format!("缺少 USD/{currency} 对应日期参考汇率")).and_then(|s|trades::scaled(s,8,false))};
 let from=rate(from)? as i128;let to=rate(to)? as i128;let n=amount as i128*to;i64::try_from((n+from/2)/from).map_err(|_|"折算金额超出支持范围".into())
}
