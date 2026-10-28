use super::{database_error,trades,Ledger,Position};
use chrono::{DateTime,Datelike,FixedOffset,NaiveDate,NaiveDateTime,NaiveTime,TimeZone,Timelike,Utc};
use rusqlite::{params,Connection,OptionalExtension,TransactionBehavior};
use serde::{Deserialize,Serialize};
use std::collections::{BTreeMap,BTreeSet};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug,Clone,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct QuoteKey{pub market:String,pub code:String,pub kind:String}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct QuoteJob{pub feed:String,pub ticket:String,pub keys:Vec<QuoteKey>}
#[derive(Debug,Serialize)]
pub struct QuotePlan{pub jobs:Vec<QuoteJob>}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct DailyClose{pub date:String,pub close:String}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct QuoteInput{#[serde(default)]pub history:Vec<DailyClose>,pub key:QuoteKey,pub price:String,pub currency:String,pub source_time:Option<String>,pub quoted_at:Option<String>}
#[derive(Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct QuoteResponse{pub feed:String,pub quotes:Vec<QuoteInput>,pub error:Option<String>}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct StoredQuote{#[serde(default)]history:Vec<DailyClose>,key:QuoteKey,price:String,currency:String,source:String,source_time:Option<String>,quoted_at:Option<String>,received_at:String}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct QuoteView{pub history:Vec<DailyClose>,pub key:QuoteKey,pub price:Option<String>,pub currency:String,pub source:String,pub source_time:Option<String>,pub quoted_at:Option<String>,pub received_at:Option<String>,pub attempted_at:Option<String>,pub status:String,pub error:Option<String>,pub retry_at:Option<String>,pub refreshing:bool,pub market_open:bool,pub delay_seconds:Option<i64>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct QuotedHolding{pub account_id:String,pub key:QuoteKey,pub quantity:String,pub cost:String,pub market_value:Option<String>,pub unrealized:Option<String>,pub valuation_error:Option<String>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct QuoteState{pub quotes:Vec<QuoteView>,pub holdings:Vec<QuotedHolding>,pub cache_warning:Option<String>,pub automatic:bool}

pub(super) struct QuoteStore{pub(super) connection:Connection,pub(super) warning:Option<String>}
fn timestamp(now:i64)->Result<String,String>{DateTime::<Utc>::from_timestamp(now,0).map(|d|d.to_rfc3339()).ok_or_else(||"系统时间无效".into())}
fn currency(key:&QuoteKey)->&'static str{if key.market=="HK"{"HKD"}else if key.market=="US"{"USD"}else{"CNY"}}
fn feed(key:&QuoteKey)->Option<&'static str>{match(key.market.as_str(),key.kind.as_str()){("CN","stock")=>Some("cn-stock"),("CN","etf")=>Some("cn-etf"),("HK","stock"|"etf")=>Some("hk"),("US","stock"|"etf")=>Some("us"),_=>None}}
fn source(feed:&str)->&str{match feed{"cn-stock"=>"AKShare stock_zh_a_spot / 新浪","cn-etf"=>"AKShare fund_etf_category_sina / 新浪","hk"=>"AKShare stock_hk_spot / 新浪","us"=>"yfinance / Yahoo Finance",_=>"尚未接入"}}
fn key_text(key:&QuoteKey)->String{format!("{}:{}:{}",key.market,key.code,key.kind)}
fn regular_open(now:i64,market:&str)->bool{
 if market=="US"{let Some(d)=DateTime::<Utc>::from_timestamp(now,0)else{return false;};let d=d.with_timezone(&chrono_tz::America::New_York);let minute=d.hour()*60+d.minute();return d.weekday().number_from_monday()<=5&&(570..960).contains(&minute);}
 if !["CN","HK"].contains(&market){return false;}
 let Some(d)=DateTime::<Utc>::from_timestamp(now,0) else{return false;};let d=d.with_timezone(&FixedOffset::east_opt(8*3600).expect("valid fixed offset"));if d.weekday().number_from_monday()>5{return false;}
 let minute=d.hour()*60+d.minute();if market=="HK"{(570..720).contains(&minute)||(780..960).contains(&minute)}else{(570..690).contains(&minute)||(780..900).contains(&minute)}
}
fn requested(positions:&[Position],probe:bool)->Vec<QuoteKey>{
 if probe{return vec![QuoteKey{market:"CN".into(),code:"600036".into(),kind:"stock".into()},QuoteKey{market:"HK".into(),code:"00700".into(),kind:"stock".into()},QuoteKey{market:"US".into(),code:"AAPL".into(),kind:"stock".into()},QuoteKey{market:"US".into(),code:"SPY".into(),kind:"etf".into()}];}
 positions.iter().filter(|p|p.quantity!="0").map(|p|QuoteKey{market:p.market.clone(),code:p.code.clone(),kind:p.kind.clone()}).collect::<BTreeSet<_>>().into_iter().collect()
}
impl QuoteStore{
 fn open(path:&Path)->Result<Self,String>{
  let connection=Connection::open(path).map_err(database_error)?;connection.busy_timeout(std::time::Duration::from_secs(3)).map_err(database_error)?;
  let version:i64=connection.pragma_query_value(None,"user_version",|r|r.get(0)).map_err(database_error)?;if version>3{return Err("行情缓存版本不兼容".into());}
  connection.execute_batch("CREATE TABLE IF NOT EXISTS market_quotes(key TEXT PRIMARY KEY,data TEXT,error TEXT,last_attempt INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS market_feeds(id TEXT PRIMARY KEY,failures INTEGER NOT NULL,next_retry INTEGER NOT NULL,ticket TEXT,request TEXT,last_attempt INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS valuation_data(key TEXT PRIMARY KEY,data TEXT,error TEXT,last_attempt INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS history_sessions(market TEXT NOT NULL,date TEXT NOT NULL,session TEXT,reason TEXT,calendar TEXT NOT NULL,version TEXT NOT NULL,PRIMARY KEY(market,date));CREATE TABLE IF NOT EXISTS history_batches(id TEXT PRIMARY KEY,last_attempt INTEGER NOT NULL);PRAGMA user_version=3;").map_err(database_error)?;Ok(Self{connection,warning:None})
 }
 pub(super) fn for_ledger(path:&Path)->Result<Self,String>{
  let cache=if path==Path::new(":memory:"){path.to_path_buf()}else{path.with_extension("quotes.sqlite3")};
  match Self::open(&cache){Ok(store)=>Ok(store),Err(_)=>{let mut store=Self::open(Path::new(":memory:"))?;store.warning=Some("行情磁盘缓存不可用，本次使用内存缓存；个人账本继续正常使用".into());Ok(store)}}
 }
 fn begin(&mut self,keys:Vec<QuoteKey>,now:i64,automatic:bool)->Result<QuotePlan,String>{
  timestamp(now)?;if keys.len()>300{return Err("单次持仓行情最多支持 300 项证券，请先整理持仓范围".into());}
  let mut grouped:BTreeMap<String,Vec<QuoteKey>>=BTreeMap::new();for key in keys{if let Some(id)=feed(&key){if !automatic||regular_open(now,&key.market){grouped.entry(id.into()).or_default().push(key);}}}
  let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;let mut jobs=Vec::new();
  for(id,mut keys)in grouped{
   if id=="us"{let mut ages=Vec::new();for key in keys{let attempted:Option<i64>=tx.query_row("SELECT last_attempt FROM market_quotes WHERE key=?1",[key_text(&key)],|r|r.get(0)).optional().map_err(database_error)?;ages.push((attempted.unwrap_or(0),key));}ages.sort();keys=ages.into_iter().take(10).map(|(_,key)|key).collect();}
   let due:Option<i64>=tx.query_row("SELECT next_retry FROM market_feeds WHERE id=?1",[&id],|r|r.get(0)).optional().map_err(database_error)?;if due.is_some_and(|t|now<t){continue;}
   let job=QuoteJob{feed:id.clone(),ticket:Uuid::new_v4().to_string(),keys};let json=serde_json::to_string(&job).map_err(|_|"无法开始行情请求")?;
   tx.execute("INSERT INTO market_feeds(id,failures,next_retry,ticket,request,last_attempt) VALUES(?1,0,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET next_retry=excluded.next_retry,ticket=excluded.ticket,request=excluded.request,last_attempt=excluded.last_attempt",params![id,now+120,job.ticket,json,now]).map_err(database_error)?;
   for key in &job.keys{tx.execute("INSERT INTO market_quotes(key,data,error,last_attempt) VALUES(?1,NULL,NULL,?2) ON CONFLICT(key) DO UPDATE SET last_attempt=excluded.last_attempt",params![key_text(key),now]).map_err(database_error)?;}jobs.push(job);
  }tx.commit().map_err(database_error)?;Ok(QuotePlan{jobs})
 }
 fn finish(&mut self,job:&QuoteJob,response:&QuoteResponse,now:i64)->Result<(),String>{
  let received=timestamp(now)?;let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
  let saved:Option<(String,String,i64)>=tx.query_row("SELECT ticket,request,failures FROM market_feeds WHERE id=?1 AND ticket IS NOT NULL",[&job.feed],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(database_error)?;
  let Some((ticket,json,failures))=saved else{return Err("行情请求已结束或过期".into());};if ticket!=job.ticket||serde_json::from_str::<QuoteJob>(&json).map_err(|_|"行情请求损坏")?!=*job{return Err("行情请求已被替换，请读取当前缓存".into());}
  let mut protocol_error=response.error.clone();let mut incoming=BTreeMap::new();
  if response.feed!=job.feed||response.quotes.len()>job.keys.len()||response.error.is_some()&&!response.quotes.is_empty(){protocol_error=Some("行情响应身份或协议不符".into());}
  for q in &response.quotes{if !job.keys.contains(&q.key)||incoming.insert(q.key.clone(),q).is_some(){protocol_error=Some("行情响应含未请求或重复证券".into());}}
  let mut valid=0;
  for key in &job.keys{
   let old:Option<String>=tx.query_row("SELECT data FROM market_quotes WHERE key=?1",[key_text(key)],|r|r.get(0)).optional().map_err(database_error)?.flatten();let old=old.and_then(|s|serde_json::from_str::<StoredQuote>(&s).ok());
   let result=if let Some(error)=&protocol_error{Err(error.clone())}else if let Some(q)=incoming.get(key){validate_quote(q,old.as_ref(),now).map(|_|StoredQuote{history:q.history.clone(),key:key.clone(),price:q.price.clone(),currency:q.currency.clone(),source:source(&job.feed).into(),source_time:q.source_time.clone(),quoted_at:q.quoted_at.clone(),received_at:received.clone()})}else{Err("来源未返回有效报价（可能停牌、缺失或接口不支持）".into())};
   match result{Ok(quote)=>{let data=serde_json::to_string(&quote).map_err(|_|"无法保存行情缓存")?;tx.execute("UPDATE market_quotes SET data=?2,error=NULL,last_attempt=?3 WHERE key=?1",params![key_text(key),data,now]).map_err(database_error)?;valid+=1;},Err(error)=>{let error:String=error.chars().take(400).collect();tx.execute("UPDATE market_quotes SET error=?2,last_attempt=?3 WHERE key=?1",params![key_text(key),error,now]).map_err(database_error)?;}}
  }
  let failures=if valid>0{0}else{(failures+1).min(5)};let delay=if failures==0{60}else{(60i64*2i64.pow((failures-1)as u32)).min(900)};
  tx.execute("UPDATE market_feeds SET failures=?2,next_retry=?3,ticket=NULL,request=NULL,last_attempt=?4 WHERE id=?1",params![job.feed,failures,now+delay,now]).map_err(database_error)?;tx.commit().map_err(database_error)?;Ok(())
 }
 pub(super) fn views(&self,keys:Vec<QuoteKey>,now:i64)->Result<Vec<QuoteView>,String>{
  timestamp(now)?;let tx=self.connection.unchecked_transaction().map_err(database_error)?;let mut views=Vec::new();
  for key in keys{
   let id=feed(&key);let row:Option<(Option<String>,Option<String>,i64)>=tx.query_row("SELECT data,error,last_attempt FROM market_quotes WHERE key=?1",[key_text(&key)],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(database_error)?;
   let mut error=row.as_ref().and_then(|r|r.1.clone());let quote=row.as_ref().and_then(|r|r.0.as_ref()).and_then(|s|serde_json::from_str::<StoredQuote>(s).ok()).filter(|q|q.key==key&&q.currency==currency(&key)&&trades::scaled(&q.price,6,false).is_ok());
   let schedule:Option<(i64,Option<String>)>=if let Some(id)=id{tx.query_row("SELECT next_retry,ticket FROM market_feeds WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(database_error)?}else{None};
   let refreshing=schedule.as_ref().is_some_and(|(lease,ticket)|ticket.is_some()&&now<*lease);let status=if id.is_none(){error=Some("该市场或证券类型尚未接入行情".into());"unsupported"}else if quote.is_none(){if error.is_none(){error=Some("尚无有效报价，请刷新或检查连接".into());}"missing"}else if error.is_some(){"stale"}else if quote.as_ref().is_some_and(|q|q.quoted_at.is_none()){"time_unknown"}else{"delayed"};
   views.push(QuoteView{history:quote.as_ref().map(|q|q.history.clone()).unwrap_or_default(),currency:currency(&key).into(),source:quote.as_ref().map(|q|q.source.clone()).unwrap_or_else(||source(id.unwrap_or("")).into()),price:quote.as_ref().map(|q|q.price.clone()),source_time:quote.as_ref().and_then(|q|q.source_time.clone()),quoted_at:quote.as_ref().and_then(|q|q.quoted_at.clone()),received_at:quote.as_ref().map(|q|q.received_at.clone()),attempted_at:row.as_ref().map(|r|timestamp(r.2)).transpose()?,status:status.into(),error,retry_at:schedule.map(|s|timestamp(s.0)).transpose()?,refreshing,market_open:regular_open(now,&key.market),delay_seconds:if key.market=="HK"{Some(900)}else{None},key});
  }tx.commit().map_err(database_error)?;Ok(views)
 }
}
fn validate_quote(q:&QuoteInput,old:Option<&StoredQuote>,now:i64)->Result<(),String>{
 if q.currency!=currency(&q.key){return Err("报价币种不符，沿用最后有效值".into());}trades::scaled(&q.price,6,false).map_err(|_|"报价缺失、为零或格式无效，沿用最后有效值")?;
 if old.is_some_and(|previous|previous.quoted_at.is_some())&&q.quoted_at.is_none(){return Err("新报价缺少完整日期，沿用已核对的旧值".into());}
 if ["HK","US"].contains(&q.key.market.as_str())&&q.quoted_at.is_none(){return Err("来源缺少完整报价日期，沿用最后有效值".into());}
 if q.source_time.as_ref().is_some_and(|s|s.len()>80){return Err("报价时间格式异常".into());}
 if let Some(quoted)=&q.quoted_at{
  let date=DateTime::parse_from_rfc3339(quoted).map_err(|_|"报价日期格式无效")?;if date.timestamp()>now+60{return Err("报价时间在未来，沿用最后有效值".into());}
  let raw=q.source_time.as_ref().ok_or("缺少原始报价时间依据")?;
  if q.key.market=="US"{let raw=DateTime::parse_from_rfc3339(raw).map_err(|_|"美股原始报价时间格式无效")?;if raw.timestamp()!=date.timestamp(){return Err("报价时间与来源原文不一致".into());}}
  else{let raw=NaiveDateTime::parse_from_str(raw,"%Y/%m/%d %H:%M:%S").map_err(|_|"原始报价时间不含可核对的完整日期")?;
  let raw=FixedOffset::east_opt(8*3600).and_then(|offset|offset.from_local_datetime(&raw).single()).ok_or("原始报价时间无效")?;if raw.timestamp()!=date.timestamp(){return Err("报价时间与来源原文不一致".into());}}
  if let Some(previous)=old.and_then(|q|q.quoted_at.as_ref()).and_then(|s|DateTime::parse_from_rfc3339(s).ok()){if date<previous{return Err("来源返回更早报价，沿用最后有效值".into());}}
 }else if let Some(raw)=&q.source_time{NaiveTime::parse_from_str(raw,"%H:%M:%S").map_err(|_|"来源时间格式无效")?;}
 if q.key.market=="US"{
  if q.history.is_empty()||q.history.len()>32{return Err("美股日线缺失或超出支持范围，沿用最后有效值".into());}
  let today=DateTime::<Utc>::from_timestamp(now,0).ok_or("系统时间无效")?.with_timezone(&chrono_tz::America::New_York).date_naive();let mut previous=None;
  for day in &q.history{let date=NaiveDate::parse_from_str(&day.date,"%Y-%m-%d").map_err(|_|"历史交易日期无效")?;if day.date!=date.format("%Y-%m-%d").to_string()||date>today||previous.is_some_and(|p|date<=p){return Err("历史日线日期重复、倒序或在未来".into());}trades::scaled(&day.close,6,false).map_err(|_|"历史收盘价无效")?;previous=Some(date);}
 }else if !q.history.is_empty(){return Err("行情响应含不支持的历史数据".into());}
 Ok(())
}
impl Ledger{
 pub fn begin_quote_refresh(&mut self,now:i64,probe:bool,automatic:bool)->Result<QuotePlan,String>{let snapshot=self.snapshot()?;self.market.begin(requested(&snapshot.positions,probe),now,automatic)}
 pub fn finish_quote_job(&mut self,job:&QuoteJob,response:&QuoteResponse,now:i64)->Result<(),String>{self.market.finish(job,response,now)}
 pub fn quote_state(&self,now:i64,probe:bool)->Result<QuoteState,String>{
  let snapshot=self.snapshot()?;let quotes=self.market.views(requested(&snapshot.positions,probe),now)?;let mut holdings=Vec::new();
  if !probe{for p in snapshot.positions.iter().filter(|p|p.quantity!="0"){
   let key=QuoteKey{market:p.market.clone(),code:p.code.clone(),kind:p.kind.clone()};let calculation=quotes.iter().find(|q|q.key==key).and_then(|q|q.price.as_ref()).map(|price|trades::value_at_position(p,price));let(value,valuation_error)=match calculation{Some(Ok(value))=>(Some(value),None),Some(Err(error))=>(None,Some(error)),None=>(None,None)};
   holdings.push(QuotedHolding{account_id:p.account_id.clone(),key,quantity:p.quantity.clone(),cost:p.cost.clone(),market_value:value.as_ref().map(|v|v.market_value.clone()),unrealized:value.map(|v|v.unrealized),valuation_error});
  }}Ok(QuoteState{quotes,holdings,cache_warning:self.market.warning.clone(),automatic:true})
 }
}
