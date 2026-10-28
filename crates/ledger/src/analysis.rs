use super::{Ledger,ValuationQuery,QuoteKey,Position,ValuationJob,backup,trades,valuation,amount_text};
use std::collections::{BTreeMap,BTreeSet};
use serde::Serialize;
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct AnalysisRow{pub account_id:String,pub account_name:String,pub key:QuoteKey,pub name:String,pub currency:String,pub industry:String,pub industry_source:Option<String>,pub quantity:String,pub cost:String,pub realized:String,pub unrealized:Option<String>,pub net_dividend:String,pub original_market_value:Option<String>,pub market_value:Option<String>,pub realized_value:Option<String>,pub unrealized_value:Option<String>,pub dividend_value:Option<String>,pub net_contribution:Option<String>,pub since:Option<String>,pub source:String,pub source_time:Option<String>,pub reasons:Vec<String>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct AllocationGroup{pub dimension:String,pub key:String,pub label:String,pub known_value:Option<String>,pub share:Option<String>,pub missing_count:usize,pub member_count:usize}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct AnalysisBar{pub key:QuoteKey,pub name:String,pub amount:Option<String>,pub share:Option<String>,pub excluded:usize}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct AnalysisContribution{pub metric:String,pub label:String,pub known_total:Option<String>,pub complete:bool,pub bars:Vec<AnalysisBar>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct AnalysisState{pub date:String,pub currency:String,pub total_assets:Option<String>,pub known_assets:Option<String>,pub asset_complete:bool,pub sources:Vec<super::ValuationSource>,pub rows:Vec<AnalysisRow>,pub allocations:Vec<AllocationGroup>,pub contributions:Vec<AnalysisContribution>,pub warnings:Vec<String>,pub missing:Vec<String>}

fn signed(text:&str)->Result<i64,String>{let(negative,text)=text.strip_prefix('-').map(|v|(true,v)).unwrap_or((false,text));let n=trades::scaled(text,2,true)?;Ok(if negative{-n}else{n})}
fn converted(text:&str,from:&str,to:&str,sources:&[super::ValuationSource])->Result<String,String>{let n=signed(text)?;let value=valuation::convert(n.checked_abs().ok_or("盈亏金额超出支持范围")?,from,to,sources)?;Ok(amount_text(if n<0{-value}else{value}))}
fn total(mut values:impl Iterator<Item=i64>)->Option<String>{values.try_fold(0i128,|s,v|s.checked_add(v as i128)).and_then(|s|i64::try_from(s).ok()).map(amount_text)}
fn share(value:i128,denominator:i128)->Option<String>{if denominator<=0{return None;}let n=(value*10000+denominator/2)/denominator;Some(format!("{}.{:02}",n/100,n%100))}

// Reviewed business categories, not a standard taxonomy or historical classification.
fn industry(key:&QuoteKey)->(&'static str,Option<&'static str>){
 if key.kind=="etf"{return("基金（未穿透）",None);}
 match(key.market.as_str(),key.code.as_str()){
  ("CN","600036")=>("银行",Some("https://www.cmbchina.com/cmbinfo/aboutcmb/")),
  ("CN","601166")=>("银行",Some("https://www.cib.com.cn/cn/aboutCIB/investor/profile/index.html")),
  ("CN","600519")=>("白酒",Some("https://www.moutaichina.com/mtgf/qygk/gsjj/index.html")),
  ("CN","600900")=>("电力",Some("https://www.cypc.com.cn/")),
  ("CN","600031")=>("工程机械",Some("https://www.sanyglobal.com/company_overview/")),
  ("CN","600660")=>("汽车零部件",Some("https://fuyaousa.com/about-2/")),
  ("HK","00700")=>("互联网服务",Some("https://www.tencent.com/who-we-are/")),
  ("US","AAPL")=>("科技硬件",Some("https://www.apple.com/business/")),
  ("US","MU")=>("半导体",Some("https://investors.micron.com/overview/")),
  ("US","AVGO")=>("半导体与基础软件",Some("https://www.broadcom.com/company/about-us")),
  ("US","ORCL")=>("软件与云服务",Some("https://www.oracle.com/corporate/")),
  _=>("未分类",None)
 }
}
fn identity(key:&QuoteKey)->String{format!("{}:{}:{}",key.market,key.code,key.kind)}
fn additional_fx(keys:&mut BTreeSet<QuoteKey>,currencies:impl Iterator<Item=String>,target:&str){let currencies=currencies.collect::<BTreeSet<_>>();if currencies.iter().any(|c|c!=target){for c in currencies.iter().map(String::as_str).chain(std::iter::once(target)).filter(|c|*c!="USD"){keys.insert(QuoteKey{market:"USD".into(),code:c.into(),kind:"fx".into()});}}}

impl Ledger{
 pub fn begin_analysis_refresh(&mut self,q:&ValuationQuery,now:i64)->Result<Option<ValuationJob>,String>{
  let frozen=backup::copy_to_memory(&self.connection)?;let(date,accounts)=frozen.valuation_accounts(q,now)?;let snapshot=frozen.snapshot()?;let mut keys=frozen.valuation_keys(q,now,false)?.into_iter().collect::<BTreeSet<_>>();let ids=accounts.iter().map(|a|a.account_id.as_str()).collect::<BTreeSet<_>>();
  let currencies=accounts.iter().filter(|a|!a.positions.is_empty()).map(|a|a.currency.clone()).chain(snapshot.dividends.iter().filter(|d|!d.deleted&&ids.contains(d.input.account_id.as_str())&&d.input.occurred_on<=date).map(|d|d.input.currency.clone()));additional_fx(&mut keys,currencies,&q.currency);self.begin_valuation_keys(q.date.clone(),keys.into_iter().collect(),now)
 }
 pub fn analysis(&self,q:&ValuationQuery,now:i64)->Result<AnalysisState,String>{
  let frozen=backup::copy_to_memory(&self.connection)?;let(date,accounts)=frozen.valuation_accounts(q,now)?;let snapshot=frozen.snapshot()?;let ids=accounts.iter().map(|a|a.account_id.clone()).collect::<BTreeSet<_>>();let info=accounts.iter().map(|a|(a.account_id.clone(),(a.name.clone(),a.unavailable_reason.clone()))).collect::<BTreeMap<_,_>>();
  let mut positions=BTreeMap::new();for a in &accounts{for p in &a.positions{positions.insert((a.account_id.clone(),QuoteKey{market:p.market.clone(),code:p.code.clone(),kind:p.kind.clone()}),p.clone());}}
  let mut dividends:BTreeMap<(String,QuoteKey),i64>=BTreeMap::new();
  for d in snapshot.dividends.iter().filter(|d|!d.deleted&&ids.contains(&d.input.account_id)&&d.input.occurred_on<=date){
   let input=&d.input;let opening=snapshot.openings.iter().find(|o|!o.deleted&&o.input.account_id==input.account_id);if opening.is_some_and(|o|input.occurred_on<=o.input.occurred_on){continue;}
   let key=QuoteKey{market:input.market.clone(),code:input.code.clone(),kind:input.kind.clone()};let id=(input.account_id.clone(),key.clone());let n=trades::scaled(&input.net,2,true)?;let current=dividends.entry(id.clone()).or_default();*current=current.checked_add(n).ok_or("分红合计超出范围，请缩小账户范围")?;
   positions.entry(id).or_insert_with(||Position{account_id:input.account_id.clone(),market:input.market.clone(),code:input.code.clone(),kind:input.kind.clone(),name:input.name.clone(),currency:input.currency.clone(),quantity:"0".into(),cost:"0.00".into(),average_cost:"0".into(),realized:"0.00".into(),since:opening.map(|o|o.input.occurred_on.clone())});
  }
  let mut keys=frozen.valuation_keys(q,now,false)?.into_iter().collect::<BTreeSet<_>>();additional_fx(&mut keys,positions.values().map(|p|p.currency.clone()),&q.currency);let sources=self.valuation_sources(keys.into_iter().collect(),q.date.as_deref(),now)?;
  let value=self.value_accounts(q,now,date.clone(),accounts,sources)?;let mut rows=Vec::new();
  for((account_id,key),position)in positions{
   let(account_name,unavailable)=info.get(&account_id).ok_or("账户范围已变化")?;let asset=value.assets.iter().find(|a|a.account_id==account_id&&a.key.as_ref()==Some(&key));let mut reasons=Vec::new();if let Some(r)=unavailable{reasons.push(r.clone());}if let Some(r)=asset.and_then(|a|a.reason.as_ref()){reasons.push(r.clone());}
   let original_market=if position.quantity=="0"{Some("0.00".into())}else{asset.and_then(|a|a.original_value.clone())};let unrealized=original_market.as_ref().map(|v|trades::scaled(v,2,true).and_then(|v|v.checked_sub(trades::scaled(&position.cost,2,true)?).ok_or_else(||"未实现盈亏溢出".into())).map(amount_text)).transpose()?;let net_dividend=amount_text(*dividends.get(&(account_id.clone(),key.clone())).unwrap_or(&0));
   let market_value=if position.quantity=="0"{Some("0.00".into())}else{asset.and_then(|a|a.converted_value.clone())};let mut realized_value=None;let mut unrealized_value=None;let mut dividend_value=None;let mut net_contribution=None;
   if market_value.is_some()&&unavailable.is_none(){let result=(||->Result<(String,String,String,String),String>{let r=converted(&position.realized,&position.currency,&q.currency,&value.sources)?;let u=converted(unrealized.as_deref().ok_or("缺少未实现盈亏")?,&position.currency,&q.currency,&value.sources)?;let d=converted(&net_dividend,&position.currency,&q.currency,&value.sources)?;let total=total([signed(&r)?,signed(&u)?,signed(&d)?].into_iter()).ok_or("证券贡献合计溢出")?;Ok((r,u,d,total))})();match result{Ok((r,u,d,n))=>{realized_value=Some(r);unrealized_value=Some(u);dividend_value=Some(d);net_contribution=Some(n);},Err(error)=>reasons.push(error)}}else if reasons.is_empty(){reasons.push("缺少证券价格或折算汇率，未参与收益贡献金额及条宽归一化".into());}
   let(sector,sector_source)=industry(&key);rows.push(AnalysisRow{account_id,account_name:account_name.clone(),key,name:position.name,currency:position.currency,industry:sector.into(),industry_source:sector_source.map(str::to_string),quantity:position.quantity,cost:position.cost,realized:position.realized,unrealized,net_dividend,original_market_value:original_market,market_value,realized_value,unrealized_value,dividend_value,net_contribution,since:position.since,source:asset.map(|a|a.source.clone()).unwrap_or_else(||"已清仓 / 仅分红记录，不需现价".into()),source_time:asset.and_then(|a|a.source_time.clone()),reasons});
  }
  let allocations=allocations(&value,&rows)?;let mut contributions=contributions(&rows)?;if info.values().any(|(_,reason)|reason.is_some()){for contribution in &mut contributions{contribution.complete=false;}}let mut warnings=value.warnings;if let Some(warning)=value.cache_warning.as_ref(){warnings.push(warning.clone());}warnings.push("分布含现金与待到账分红；占比只按已知资产计算，缺项保留在图例中".into());warnings.push("证券贡献为各账户期初收盘以后累计：已实现 + 未实现 + 已确认税后分红（含应收）；分红到账不再次计入。原币盈亏按所选估值时点汇率逐项折算，非证券转账费用及现金/实际换汇汇兑盈亏不在此证券贡献内，不等于完整组合净收益或收益率贡献".into());warnings.push("行业为本地已核对业务分类（2026-10-27），非标准行业体系或历史行业分类；基金不穿透，未覆盖证券明确列为未分类".into());
  Ok(AnalysisState{date,currency:q.currency.clone(),total_assets:value.total,known_assets:value.known_total,asset_complete:value.complete,sources:value.sources,rows,allocations,contributions,warnings,missing:value.missing})
 }
}
#[derive(Default)]
struct AllocationBucket{label:String,sum:i128,known:usize,missing:usize,members:usize}
fn allocations(value:&super::ValuationState,rows:&[AnalysisRow])->Result<Vec<AllocationGroup>,String>{
 let mut buckets:BTreeMap<(String,String),AllocationBucket>=BTreeMap::new();
 for asset in &value.assets{
  let row=asset.key.as_ref().and_then(|k|rows.iter().find(|r|r.account_id==asset.account_id&&r.key==*k));let(market,market_label)=match asset.key.as_ref().map(|k|k.market.as_str()){Some("CN")=>("CN","A 股"),Some("HK")=>("HK","港股"),Some("US")=>("US","美股"),_=>("cash","现金与应收")};let sector=row.map(|r|r.industry.as_str()).unwrap_or("现金与应收");let security=asset.key.as_ref().map(identity).unwrap_or_else(||"cash".into());let security_label=row.map(|r|format!("{}:{} {}",r.key.market,r.key.code,r.name)).unwrap_or_else(||"现金与应收".into());
  for(dimension,key,label)in [("market",market.to_string(),market_label.to_string()),("currency",asset.currency.clone(),asset.currency.clone()),("industry",sector.to_string(),sector.to_string()),("security",security,security_label)]{let bucket=buckets.entry((dimension.into(),key)).or_default();bucket.label=label;bucket.members+=1;if let Some(v)=asset.converted_value.as_ref(){bucket.sum+=signed(v)? as i128;bucket.known+=1;}else{bucket.missing+=1;}}
 }
 let denominator=value.known_total.as_deref().map(signed).transpose()?.unwrap_or(0)as i128;
 Ok(buckets.into_iter().map(|((dimension,key),b)|{let known=if b.known==0{None}else{i64::try_from(b.sum).ok().map(amount_text)};AllocationGroup{dimension,key,label:b.label,share:if known.is_some(){share(b.sum,denominator)}else{None},known_value:known,missing_count:b.missing,member_count:b.members}}).collect())
}
fn contributions(rows:&[AnalysisRow])->Result<Vec<AnalysisContribution>,String>{
 let mut result=Vec::new();for(metric,label)in [("net","证券净贡献"),("unrealized","未实现盈亏"),("realized","已实现盈亏"),("dividend","税后分红贡献")]{
  let mut grouped:BTreeMap<QuoteKey,(String,i128,usize,usize)>=BTreeMap::new();for row in rows{let value=match metric{"net"=>&row.net_contribution,"realized"=>&row.realized_value,"unrealized"=>&row.unrealized_value,_=>&row.dividend_value};let b=grouped.entry(row.key.clone()).or_insert_with(||(row.name.clone(),0,0,0));if let Some(value)=value{b.1+=signed(value)?as i128;b.2+=1;}else{b.3+=1;}}
  let mut bars=grouped.into_iter().map(|(key,(name,sum,known,excluded))|AnalysisBar{key,name,amount:if known>0{i64::try_from(sum).ok().map(amount_text)}else{None},share:None,excluded}).collect::<Vec<_>>();let amounts=bars.iter().filter_map(|b|b.amount.as_deref()).map(signed).collect::<Result<Vec<_>,_>>()?;let denominator=amounts.iter().map(|v|(*v as i128).abs()).sum::<i128>();for bar in &mut bars{bar.share=bar.amount.as_deref().map(signed).transpose()?.and_then(|n|share((n as i128).abs(),denominator));}let known_total=if amounts.is_empty(){None}else{total(amounts.into_iter())};let complete=!bars.is_empty()&&known_total.is_some()&&bars.iter().all(|b|b.amount.is_some()&&b.excluded==0);result.push(AnalysisContribution{metric:metric.into(),label:label.into(),known_total,complete,bars});
 }Ok(result)
}
