use super::{backup,database_error,snapshot_from,trades,validate_record_fields,Ledger,Snapshot,TradeInput,MovementInput,DividendInput,OpeningInput};
use rusqlite::{params,Connection,OptionalExtension,TransactionBehavior};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::collections::{BTreeMap,HashSet};
use chrono::Utc;
use uuid::Uuid;

pub const CSV_HEADER:&str="record_id,type,account_id,date,currency,amount,market,code,kind,name,quantity,price,fee,tax,to_account_id,to_currency,to_amount,rate,fee_side,status,paid_on,net,positions,note";
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct CashInput{pub account_id:String,pub occurred_on:String,pub currency:String,pub amount:String,pub note:String}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(tag="type",content="input",rename_all="lowercase",deny_unknown_fields)]
pub enum ImportRecord{Deposit(CashInput),Withdrawal(CashInput),Buy(TradeInput),Sell(TradeInput),Dividend(DividendInput),Transfer(MovementInput),Exchange(MovementInput),Opening(OpeningInput)}
impl ImportRecord{
 fn kind(&self)->&str{match self{Self::Deposit(_)=>"deposit",Self::Withdrawal(_)=>"withdrawal",Self::Buy(_)=>"buy",Self::Sell(_)=>"sell",Self::Dividend(_)=>"dividend",Self::Transfer(_)=>"transfer",Self::Exchange(_)=>"exchange",Self::Opening(_)=>"opening"}}
 fn account(&self)->&str{match self{Self::Deposit(t)|Self::Withdrawal(t)=>&t.account_id,Self::Buy(t)|Self::Sell(t)=>&t.account_id,Self::Dividend(t)=>&t.account_id,Self::Transfer(t)|Self::Exchange(t)=>&t.from_account_id,Self::Opening(t)=>&t.account_id}}
 fn table(&self)->&str{match self{Self::Deposit(_)=>"deposits",Self::Withdrawal(_)=>"",Self::Buy(_)|Self::Sell(_)=>"trades",Self::Dividend(_)=>"dividends",Self::Transfer(_)|Self::Exchange(_)=>"movements",Self::Opening(_)=>"openings"}}
 fn data(&self)->Result<String,String>{let value=serde_json::to_value(self).map_err(|_|"无法保存导入内容")?;serde_json::to_string(&value["input"]).map_err(|_|"无法保存导入内容".into())}
 fn accounts(&self)->Vec<&str>{match self{Self::Transfer(t)|Self::Exchange(t)=>vec![&t.from_account_id,&t.to_account_id],_=>vec![self.account()]}}
}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ImportChange{pub at:String,pub action:String,pub record:ImportRecord,pub deleted:bool,pub csv:String}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ImportItem{pub id:String,pub batch_id:String,pub external_id:String,pub account_id:String,pub record_type:String,pub record_id:String,pub event_order:i64,pub line:usize,pub raw:String,pub original:ImportRecord,pub current:ImportRecord,pub current_csv:String,pub deleted:bool,pub history:Vec<ImportChange>}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ImportBatch{pub id:String,pub created_at:String,pub raw_csv:String,pub withdrawn_at:Option<String>}
#[derive(Debug,Default,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ImportArchive{pub batches:Vec<ImportBatch>,pub items:Vec<ImportItem>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ImportLog{pub fingerprint:String,pub batches:Vec<ImportBatch>,pub items:Vec<ImportItem>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct CsvRow{pub line:usize,pub raw:String,pub external_id:String,pub record_type:String,pub account_id:String,pub date:String,pub currency:String,pub fields:BTreeMap<String,String>,pub status:String,pub error:Option<String>,pub record:Option<ImportRecord>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct CsvPreview{pub fingerprint:String,pub rows:Vec<CsvRow>,pub new_count:usize,pub duplicate_count:usize,pub can_commit:bool,pub error:Option<String>}
#[derive(Debug,Serialize)]
pub struct ImportResult{pub snapshot:Snapshot,pub log:ImportLog}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct BatchUndoPreview{pub fingerprint:String,pub items:Vec<ImportItem>,pub account_ids:Vec<String>,pub can_undo:bool,pub error:Option<String>}

struct ParsedRow{line:usize,raw:String,fields:Vec<String>}
fn parse_csv(text:&str)->Result<Vec<ParsedRow>,String>{
 if text.len()>2*1024*1024{return Err("CSV 超过 2 MiB 上限".into());}
 let text=text.strip_prefix('\u{feff}').unwrap_or(text);let mut rows=Vec::new();let mut values=Vec::new();let mut field=String::new();let(mut quoted,mut closed)=(false,false);let(mut line,mut start_line,mut start)=(1,1,0);let mut chars=text.char_indices().peekable();
 while let Some((index,ch))=chars.next(){
  if quoted{if ch=='"'{if chars.peek().is_some_and(|(_,c)|*c=='"'){chars.next();field.push('"');}else{quoted=false;closed=true;}}else{field.push(ch);if ch=='\n'{line+=1;}}continue;}
  match ch{
   '"' if field.is_empty()&&!closed=>quoted=true,
   ','=>{values.push(std::mem::take(&mut field));closed=false;},
   '\n'|'\r'=>{let mut end=index+ch.len_utf8();if ch=='\r'&&chars.peek().is_some_and(|(_,c)|*c=='\n'){let(i,_)=chars.next().ok_or("CSV 换行错误")?;end=i+1;}values.push(std::mem::take(&mut field));if !(values.len()==1&&values[0].is_empty()){rows.push(ParsedRow{line:start_line,raw:text[start..index].into(),fields:std::mem::take(&mut values)});}else{values.clear();}line+=1;start_line=line;start=end;closed=false;},
   '"'=>return Err(format!("第 {line} 行引号必须从字段起始处开始")),
   _ if closed=>return Err(format!("第 {line} 行闭合引号后只允许逗号或换行")),
   _=>field.push(ch),
  }
  if rows.len()>2001{return Err("CSV 最多支持 2000 数据行".into());}
 }
 if quoted{return Err(format!("第 {start_line} 行引号未闭合"));}
 if start<text.len(){values.push(field);rows.push(ParsedRow{line:start_line,raw:text[start..].into(),fields:values});}
 if rows.len()<2||rows.len()>2001{return Err("CSV 需包含表头及 1—2000 条记录".into());}Ok(rows)
}
fn canonical_number(text:String,digits:usize)->String{
 let(whole,fraction)=text.split_once('.').unwrap_or((&text,""));
 if fraction.len()>digits||whole.is_empty()||!whole.bytes().all(|b|b.is_ascii_digit())||!fraction.bytes().all(|b|b.is_ascii_digit())||(text.contains('.')&&fraction.is_empty()){return text;}
 let whole=whole.trim_start_matches('0');let whole=if whole.is_empty(){"0"}else{whole};let fraction=fraction.trim_end_matches('0');if fraction.is_empty(){whole.into()}else{format!("{whole}.{fraction}")}
}
fn record_from(fields:&BTreeMap<String,String>)->Result<ImportRecord,String>{
 let get=|key:&str|{let text=fields.get(key).cloned().unwrap_or_default();if ["amount","quantity","price","fee","tax","net","to_amount","rate"].contains(&key){canonical_number(text,if key=="rate"{8}else if ["quantity","price"].contains(&key){6}else{2})}else{text}};let value=|key:&str,default:&str|{let s=get(key);if s.is_empty(){default.into()}else{s}};
 let record_type=get("type");let common=["record_id","type","account_id","date","currency","note"];
 let allowed:&[&str]=match record_type.as_str(){"deposit"|"withdrawal"=>&["amount"],"buy"|"sell"=>&["market","code","kind","name","quantity","price","fee","tax"],"dividend"=>&["market","code","kind","name","amount","tax","status","paid_on","net"],"transfer"|"exchange"=>&["amount","to_account_id","to_currency","to_amount","rate","fee","fee_side"],"opening"=>&["amount","positions"],_=>return Err(format!("未知记录类型 {record_type}，请修正后重新预览"))};
 if let Some((key,_))=fields.iter().find(|(key,v)|!v.is_empty()&&!common.contains(&key.as_str())&&!allowed.contains(&key.as_str())){return Err(format!("{record_type} 不使用字段 {key}，请核对类型或清空该值"));}
 let external=get("record_id");if external.is_empty()||external.chars().count()>128{return Err("record_id 应为 1—128 字符的稳定外部记录号".into());}
 let account_id=get("account_id");let occurred_on=get("date");let note=get("note");let currency=get("currency");validate_record_fields(&occurred_on,&note)?;
 if !["CNY","USD","HKD"].contains(&currency.as_str()){return Err("currency 必须为 CNY、USD 或 HKD".into());}
 let result=match record_type.as_str(){
  "deposit"|"withdrawal"=>{let t=CashInput{account_id,occurred_on,note,currency,amount:get("amount")};trades::scaled(&t.amount,2,false)?;if record_type=="deposit"{ImportRecord::Deposit(t)}else{ImportRecord::Withdrawal(t)}},
  "buy"|"sell"=>{let t=TradeInput{account_id,occurred_on,note,currency,market:get("market"),code:get("code"),kind:get("kind"),name:get("name"),side:record_type.clone(),quantity:get("quantity"),price:get("price"),fee:value("fee","0"),tax:value("tax","0")};trades::validate_input(&t)?;if record_type=="buy"{ImportRecord::Buy(t)}else{ImportRecord::Sell(t)}},
  "dividend"=>{let t=DividendInput{account_id,occurred_on,note,currency,market:get("market"),code:get("code"),kind:get("kind"),name:get("name"),gross:get("amount"),tax:value("tax","0"),net:get("net"),status:get("status"),paid_on:if get("paid_on").is_empty(){None}else{Some(get("paid_on"))}};super::dividends::validate_values(&t)?;ImportRecord::Dividend(t)},
  "transfer"|"exchange"=>{let t=MovementInput{from_account_id:account_id,to_account_id:get("to_account_id"),occurred_on,note,kind:record_type.clone(),from_currency:currency,to_currency:get("to_currency"),from_amount:get("amount"),to_amount:get("to_amount"),actual_rate:get("rate"),fee:value("fee","0"),fee_side:value("fee_side","from")};super::movements::validate_values(&t)?;if record_type=="transfer"{ImportRecord::Transfer(t)}else{ImportRecord::Exchange(t)}},
  "opening"=>{let t=OpeningInput{account_id,occurred_on,note,cash:get("amount"),positions:serde_json::from_str(&value("positions","[]")).map_err(|_|"positions 必须是持仓 JSON 数组")?};super::opening::validate_input(&t)?;if t.positions.iter().any(|p|p.currency!=currency){return Err("期初持仓原币与 currency 不一致".into());}ImportRecord::Opening(t)},
  _=>return Err("未知记录类型".into()),
 };Ok(result)
}
fn validate_accounts(connection:&Connection,record:&ImportRecord,currency:&str,new:bool)->Result<(),String>{
 for id in record.accounts(){let(c,archived):(String,bool)=connection.query_row("SELECT currency,archived FROM accounts WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(database_error)?.ok_or("账户 ID 不存在，请先建账并使用准确 ID")?;
  let expected=match record{ImportRecord::Transfer(t)|ImportRecord::Exchange(t) if id==t.to_account_id=>t.to_currency.as_str(),_=>currency};
  if c!=expected{return Err("记录原币与账户币种不一致".into());}if new&&archived{return Err("归档账户不能导入新记录".into());}
 }Ok(())
}
fn digest(text:&str)->String{Sha256::digest(text.as_bytes()).iter().map(|b|format!("{b:02x}")).collect()}
pub(super) fn load_archive(connection:&Connection)->Result<ImportArchive,String>{
 fn load<T:serde::de::DeserializeOwned>(c:&Connection,table:&str)->Result<Vec<T>,String>{let mut q=c.prepare(&format!("SELECT data FROM {table} ORDER BY rowid")).map_err(database_error)?;let rows=q.query_map([],|r|r.get::<_,String>(0)).map_err(database_error)?.collect::<Result<Vec<_>,_>>().map_err(database_error)?;rows.into_iter().map(|s|serde_json::from_str(&s).map_err(|_|"导入来源损坏，请保留账本".into())).collect()}
 Ok(ImportArchive{batches:load(connection,"import_batches")?,items:load(connection,"import_items")?})
}
pub(super) fn replace_archive(connection:&Connection,archive:&ImportArchive)->Result<(),String>{
 for b in &archive.batches{connection.execute("INSERT INTO import_batches(id,data) VALUES(?1,?2)",params![b.id,serde_json::to_string(b).map_err(|_|"无法保存导入批次")?]).map_err(database_error)?;}
 for i in &archive.items{connection.execute("INSERT INTO import_items(id,batch_id,account_id,external_id,record_type,record_id,data) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![i.id,i.batch_id,i.account_id,i.external_id,i.record_type,i.record_id,serde_json::to_string(i).map_err(|_|"无法保存导入来源")?]).map_err(database_error)?;}Ok(())
}
fn log_from(connection:&Connection)->Result<ImportLog,String>{let a=load_archive(connection)?;Ok(ImportLog{fingerprint:backup::state_fingerprint(connection)?,batches:a.batches,items:a.items})}
fn token(connection:&Connection,text:&str)->Result<String,String>{Ok(digest(&format!("{}:{text}",backup::state_fingerprint(connection)?)))}
fn decoded(text:&str)->Result<Vec<CsvRow>,String>{
 let parsed=parse_csv(text)?;let header=parsed[0].fields.clone();let names:HashSet<_>=header.iter().map(String::as_str).collect();
 if names.len()!=header.len()||header.iter().any(|h|!CSV_HEADER.split(',').any(|known|known==h))||["record_id","type","account_id","date","currency"].iter().any(|key|!names.contains(key)){return Err("表头重复、未知或缺少 record_id/type/account_id/date/currency，请使用统一格式".into());}
 let mut rows=Vec::new();
 for r in parsed.into_iter().skip(1){let fields:BTreeMap<_,_>=header.iter().cloned().zip(r.fields.iter().map(|s|s.trim().to_string())).collect();let get=|key:&str|fields.get(key).cloned().unwrap_or_default();
  let mut row=CsvRow{line:r.line,raw:r.raw,external_id:get("record_id"),record_type:get("type"),account_id:get("account_id"),date:get("date"),currency:get("currency"),fields:fields.clone(),status:"new".into(),error:None,record:None};
  let result=if r.fields.len()!=header.len(){Err("字段数量与表头不符".into())}else{record_from(&fields)};
  match result{Err(e)=>{row.status="error".into();row.error=Some(e);},Ok(record)=>row.record=Some(record)}rows.push(row);
 }Ok(rows)
}
fn prepare(connection:&Connection,text:&str)->Result<CsvPreview,String>{
 let mut rows=decoded(text)?;let archive=load_archive(connection)?;let mut seen:BTreeMap<(String,String),ImportRecord>=archive.items.iter().map(|i|((i.account_id.clone(),i.external_id.clone()),i.original.clone())).collect();
 for row in &mut rows{if let Some(record)=&row.record{
  if let Err(e)=validate_accounts(connection,record,&row.currency,false){row.status="error".into();row.error=Some(e);continue;}
  let key=(record.account().to_string(),row.external_id.clone());
  let error=if let Some(original)=seen.get(&key){if original==record{row.status="duplicate".into();None}else{Some("同一账户 record_id 已存在不同原始内容，请使用纠错入口".into())}}
   else if let Err(e)=validate_accounts(connection,record,&row.currency,true){Some(e)}else{seen.insert(key,record.clone());None};
  if let Some(error)=error{row.status="error".into();row.error=Some(error);}
 }}
 let new_count=rows.iter().filter(|r|r.status=="new").count();let duplicate_count=rows.iter().filter(|r|r.status=="duplicate").count();let mut error=None;
 if !rows.iter().any(|r|r.error.is_some())&&new_count>0{
  let mut candidate=backup::copy_to_memory(connection)?;let tx=candidate.connection.transaction().map_err(database_error)?;
  if let Err(e)=apply_new(&tx,text,&rows).and_then(|_|snapshot_from(&tx).map(|_|())){error=Some(format!("整批账务校验失败：{e}"));}
 }
 let can_commit=new_count>0&&error.is_none()&&!rows.iter().any(|r|r.error.is_some());Ok(CsvPreview{fingerprint:token(connection,text)?,rows,new_count,duplicate_count,can_commit,error})
}
fn write_model(connection:&Connection,item:&ImportItem,new:bool,action:&str)->Result<(),String>{
 let record=&item.current;let id=&item.record_id;
 if let ImportRecord::Withdrawal(_)=record{return Ok(());}
 if let ImportRecord::Deposit(t)=record{
  let amount=trades::scaled(&t.amount,2,false)?;
  if new{connection.execute("INSERT INTO deposits(id,account_id,occurred_on,amount_cents,note,source,deleted) VALUES(?1,?2,?3,?4,?5,'CSV 导入',?6)",params![id,t.account_id,t.occurred_on,amount,t.note,item.deleted]).map_err(database_error)?;}
  else{connection.execute("UPDATE deposits SET occurred_on=?2,amount_cents=?3,note=?4,deleted=?5 WHERE id=?1",params![id,t.occurred_on,amount,t.note,item.deleted]).map_err(database_error)?;}
  super::save_revision(connection,id,action)?;return Ok(());
 }
 let table=record.table();let data=record.data()?;
 if new{match record{
  ImportRecord::Transfer(t)|ImportRecord::Exchange(t)=>{connection.execute("INSERT INTO movements(id,from_account_id,to_account_id,data,source,deleted,event_order) VALUES(?1,?2,?3,?4,'CSV 导入',?5,?6)",params![id,t.from_account_id,t.to_account_id,data,item.deleted,item.event_order]).map_err(database_error)?;},
  ImportRecord::Opening(t)=>{connection.execute("INSERT INTO openings(id,account_id,data,source,deleted) VALUES(?1,?2,?3,'CSV 导入',?4)",params![id,t.account_id,data,item.deleted]).map_err(database_error)?;},
  _=>{connection.execute(&format!("INSERT INTO {table}(id,account_id,data,source,deleted,event_order) VALUES(?1,?2,?3,'CSV 导入',?4,?5)"),params![id,item.account_id,data,item.deleted,item.event_order]).map_err(database_error)?;},
 }}else{connection.execute(&format!("UPDATE {table} SET data=?2,deleted=?3 WHERE id=?1"),params![id,data,item.deleted]).map_err(database_error)?;}
 let(prefix,history)=match record{ImportRecord::Buy(_)|ImportRecord::Sell(_)=>("trade","trade_revisions"),ImportRecord::Dividend(_)=>("dividend","dividend_revisions"),ImportRecord::Transfer(_)|ImportRecord::Exchange(_)=>("movement","movement_revisions"),ImportRecord::Opening(_)=>("opening","opening_revisions"),_=>return Err("未知导入记录".into())};
 connection.execute(&format!("INSERT INTO {history}({prefix}_id,data,deleted,action) VALUES(?1,?2,?3,?4)"),params![id,data,item.deleted,action]).map_err(database_error)?;Ok(())
}
fn save_item(connection:&Connection,item:&ImportItem,new:bool)->Result<(),String>{
 if new{replace_archive(connection,&ImportArchive{batches:Vec::new(),items:vec![item.clone()]})?;}else{connection.execute("UPDATE import_items SET data=?2 WHERE id=?1",params![item.id,serde_json::to_string(item).map_err(|_|"无法保存导入历史")?]).map_err(database_error)?;}Ok(())
}
fn apply_new(connection:&Connection,text:&str,rows:&[CsvRow])->Result<(),String>{
 let batch=ImportBatch{id:Uuid::new_v4().to_string(),created_at:Utc::now().to_rfc3339(),raw_csv:text.into(),withdrawn_at:None};replace_archive(connection,&ImportArchive{batches:vec![batch.clone()],items:Vec::new()})?;
 let header=parse_csv(text)?[0].raw.clone();
 for row in rows.iter().filter(|r|r.status=="new"){
  let record=row.record.clone().ok_or("记录未识别")?;let csv=format!("{header}\n{}\n",row.raw);let mut item=ImportItem{id:Uuid::new_v4().to_string(),batch_id:batch.id.clone(),external_id:row.external_id.clone(),account_id:row.account_id.clone(),record_type:row.record_type.clone(),record_id:Uuid::new_v4().to_string(),event_order:super::movements::next_event_order(connection)?,line:row.line,raw:row.raw.clone(),original:record.clone(),current:record.clone(),current_csv:csv.clone(),deleted:false,history:Vec::new()};
  item.history.push(ImportChange{at:Utc::now().to_rfc3339(),action:"CSV 导入".into(),record,deleted:false,csv});write_model(connection,&item,true,"CSV 导入")?;save_item(connection,&item,true)?;
 }Ok(())
}
pub(super) fn ensure_manual(connection:&Connection,table:&str,id:&str)->Result<(),String>{let source:String=connection.query_row(&format!("SELECT source FROM {table} WHERE id=?1"),[id],|r|r.get(0)).map_err(database_error)?;if source=="CSV 导入"{return Err("CSV 导入记录请到“CSV 导入”页面纠错或撤回".into());}Ok(())}
pub(super) fn withdrawals(connection:&Connection)->Result<Vec<(String,String,i64,i64)>,String>{Ok(load_archive(connection)?.items.into_iter().filter(|i|!i.deleted).filter_map(|i|if let ImportRecord::Withdrawal(t)=i.current{Some(trades::scaled(&t.amount,2,false).map(|n|(t.account_id,t.occurred_on,-n,i.event_order)))}else{None}).collect::<Result<Vec<_>,_>>()?)}
impl Ledger{
 pub fn import_log(&self)->Result<ImportLog,String>{let tx=self.connection.unchecked_transaction().map_err(database_error)?;let log=log_from(&tx)?;tx.commit().map_err(database_error)?;Ok(log)}
 pub fn preview_csv(&self,text:&str)->Result<CsvPreview,String>{let tx=self.connection.unchecked_transaction().map_err(database_error)?;let preview=prepare(&tx,text)?;tx.commit().map_err(database_error)?;Ok(preview)}
 pub fn commit_csv(&mut self,text:&str,expected:&str)->Result<ImportResult,String>{
  let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;if token(&tx,text)?!=expected{return Err("文件或账本已变化，请重新预览".into());}let preview=prepare(&tx,text)?;if !preview.can_commit{return Err(preview.error.unwrap_or_else(||"存在错误或全部重复，未导入；请重新预览并修正".into()));}
  apply_new(&tx,text,&preview.rows)?;let result=ImportResult{snapshot:snapshot_from(&tx)?,log:log_from(&tx)?};tx.commit().map_err(database_error)?;Ok(result)
 }
}

fn single(text:&str)->Result<CsvRow,String>{let mut rows=decoded(text)?;if rows.len()!=1{return Err("修正需要表头及恰好一条记录".into());}let row=rows.remove(0);if let Some(error)=&row.error{return Err(error.clone());}Ok(row)}
fn same_identity(a:&ImportRecord,b:&ImportRecord)->bool{a.kind()==b.kind()&&a.accounts()==b.accounts()}
fn change_item(connection:&Connection,item:&mut ImportItem,record:ImportRecord,deleted:bool,csv:&str,action:&str)->Result<(),String>{
 if !same_identity(&item.original,&record){return Err("不能更换记录类型或账户（含转入端）；请作废后以新的记录号重录".into());}
 item.current=record.clone();item.current_csv=csv.into();item.deleted=deleted;item.history.push(ImportChange{at:Utc::now().to_rfc3339(),action:action.into(),record,deleted,csv:csv.into()});write_model(connection,item,false,action)?;save_item(connection,item,false)
}
fn check_fingerprint(connection:&Connection,expected:&str)->Result<(),String>{if backup::state_fingerprint(connection)?!=expected{return Err("账本已变化，请刷新导入来源后重新选择".into());}Ok(())}
fn finish_import(tx:rusqlite::Transaction<'_>)->Result<ImportResult,String>{let result=ImportResult{snapshot:snapshot_from(&tx)?,log:log_from(&tx)?};tx.commit().map_err(database_error)?;Ok(result)}
fn batch_items(connection:&Connection,id:&str)->Result<Vec<ImportItem>,String>{let archive=load_archive(connection)?;if !archive.batches.iter().any(|b|b.id==id){return Err("批次不存在，请刷新".into());}Ok(archive.items.into_iter().filter(|i|i.batch_id==id&&!i.deleted).collect())}
fn undo_batch(connection:&Connection,id:&str,items:&mut[ImportItem])->Result<(),String>{
 if items.is_empty(){return Err("该批次没有有效记录可撤回".into());}
 for item in items{let record=item.current.clone();let csv=item.current_csv.clone();change_item(connection,item,record,true,&csv,"批次撤回")?;}
 let mut batch=load_archive(connection)?.batches.into_iter().find(|b|b.id==id).ok_or("批次不存在")?;batch.withdrawn_at=Some(Utc::now().to_rfc3339());connection.execute("UPDATE import_batches SET data=?2 WHERE id=?1",params![id,serde_json::to_string(&batch).map_err(|_|"无法保存批次撤回来源")?]).map_err(database_error)?;Ok(())
}
impl Ledger{
 pub fn revise_import_item(&mut self,id:&str,text:&str,expected:&str)->Result<ImportResult,String>{
  let row=single(text)?;let record=row.record.ok_or("记录未识别")?;let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;check_fingerprint(&tx,expected)?;
  let mut item=load_archive(&tx)?.items.into_iter().find(|i|i.id==id).ok_or("导入记录不存在，请刷新")?;if item.deleted{return Err("请先恢复导入记录再修改".into());}if row.external_id!=item.external_id{return Err("不能更换外部 record_id".into());}
  validate_accounts(&tx,&record,&row.currency,false)?;change_item(&tx,&mut item,record,false,text,"手动修正 CSV")?;finish_import(tx)
 }
 pub fn set_import_item_deleted(&mut self,id:&str,expected:&str,deleted:bool)->Result<ImportResult,String>{
  let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;check_fingerprint(&tx,expected)?;let mut item=load_archive(&tx)?.items.into_iter().find(|i|i.id==id).ok_or("导入记录不存在，请刷新")?;
  if item.deleted==deleted{return Err("记录状态已变化，请刷新".into());}let record=item.current.clone();let csv=item.current_csv.clone();change_item(&tx,&mut item,record,deleted,&csv,if deleted{"手动作废 CSV"}else{"手动恢复 CSV"})?;finish_import(tx)
 }
 pub fn preview_import_batch_undo(&self,id:&str)->Result<BatchUndoPreview,String>{
  let tx=self.connection.unchecked_transaction().map_err(database_error)?;let items=batch_items(&tx,id)?;let account_ids:HashSet<String>=items.iter().flat_map(|i|i.current.accounts().into_iter().map(str::to_string)).collect();
  let mut candidate=backup::copy_to_memory(&tx)?;let candidate_tx=candidate.connection.transaction().map_err(database_error)?;let mut simulated=items.clone();let error=undo_batch(&candidate_tx,id,&mut simulated).and_then(|_|snapshot_from(&candidate_tx).map(|_|())).err();let mut account_ids:Vec<_>=account_ids.into_iter().collect();account_ids.sort();
  let preview=BatchUndoPreview{fingerprint:token(&tx,&format!("undo:{id}"))?,items,account_ids,can_undo:error.is_none(),error};tx.commit().map_err(database_error)?;Ok(preview)
 }
 pub fn undo_import_batch(&mut self,id:&str,expected:&str)->Result<ImportResult,String>{
  let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;if token(&tx,&format!("undo:{id}"))?!=expected{return Err("批次或账本已变化，请重新预览撤回".into());}let mut items=batch_items(&tx,id)?;undo_batch(&tx,id,&mut items)?;finish_import(tx)
 }
}
pub(super) fn validate_archive(connection:&Connection)->Result<(),String>{
 let archive=load_archive(connection)?;let invalid="导入批次、来源或历史不完整";let mut targets=HashSet::new();let valid_id=|s:&str|!s.is_empty()&&s.len()<=128;
 for (batch_index,b) in archive.batches.iter().enumerate(){
  if !valid_id(&b.id)||!archive.items.iter().any(|i|i.batch_id==b.id){return Err(invalid.into());}chrono::DateTime::parse_from_rfc3339(&b.created_at).map_err(|_|invalid)?;if let Some(date)=&b.withdrawn_at{chrono::DateTime::parse_from_rfc3339(date).map_err(|_|invalid)?;}
  let rows=decoded(&b.raw_csv)?;if rows.iter().any(|r|r.error.is_some()){return Err(invalid.into());}
  for row in &rows{
   let item=archive.items.iter().find(|i|i.account_id==row.account_id&&i.external_id==row.external_id).ok_or(invalid)?;
   if row.record.as_ref()!=Some(&item.original)||archive.batches.iter().position(|other|other.id==item.batch_id).is_none_or(|origin|origin>batch_index){return Err(invalid.into());}
  }
  for i in archive.items.iter().filter(|i|i.batch_id==b.id){
   if !valid_id(&i.id)||!valid_id(&i.record_id)||i.event_order<=0||i.record_type!=i.original.kind()||i.account_id!=i.original.account()||!same_identity(&i.original,&i.current){return Err(invalid.into());}
   let row=rows.iter().find(|r|r.line==i.line).ok_or(invalid)?;if row.raw!=i.raw||row.external_id!=i.external_id||row.record.as_ref()!=Some(&i.original){return Err(invalid.into());}validate_accounts(connection,&i.original,&row.currency,false)?;
   let current=single(&i.current_csv)?;if current.external_id!=i.external_id||current.record.as_ref()!=Some(&i.current){return Err(invalid.into());}validate_accounts(connection,&i.current,&current.currency,false)?;
   let first=i.history.first().ok_or(invalid)?;let last=i.history.last().ok_or(invalid)?;if first.record!=i.original||first.deleted||last.record!=i.current||last.deleted!=i.deleted||last.csv!=i.current_csv{return Err(invalid.into());}
   for h in &i.history{chrono::DateTime::parse_from_rfc3339(&h.at).map_err(|_|invalid)?;if h.action.is_empty()||h.action.chars().count()>80||!same_identity(&i.original,&h.record){return Err(invalid.into());}let row=single(&h.csv)?;if row.external_id!=i.external_id||row.record.as_ref()!=Some(&h.record){return Err(invalid.into());}validate_accounts(connection,&h.record,&row.currency,false)?;}
   if i.current.table().is_empty(){continue;}
   let table=i.current.table();if !targets.insert((table.to_string(),i.record_id.clone())){return Err(invalid.into());}
   let(source,deleted):(String,bool)=connection.query_row(&format!("SELECT source,deleted FROM {table} WHERE id=?1"),[&i.record_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(database_error)?;if source!="CSV 导入"||deleted!=i.deleted{return Err(invalid.into());}
   if let ImportRecord::Deposit(t)=&i.current{let(date,amount,note,account):(String,i64,String,String)=connection.query_row("SELECT occurred_on,amount_cents,note,account_id FROM deposits WHERE id=?1",[&i.record_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(database_error)?;if date!=t.occurred_on||amount!=trades::scaled(&t.amount,2,false)?||note!=t.note||account!=t.account_id{return Err(invalid.into());}}
   else{let data:String=connection.query_row(&format!("SELECT data FROM {table} WHERE id=?1"),[&i.record_id],|r|r.get(0)).map_err(database_error)?;if serde_json::from_str::<serde_json::Value>(&data).map_err(|_|invalid)?!=serde_json::from_str::<serde_json::Value>(&i.current.data()?).map_err(|_|invalid)?{return Err(invalid.into());}if table!="openings"{let order:i64=connection.query_row(&format!("SELECT event_order FROM {table} WHERE id=?1"),[&i.record_id],|r|r.get(0)).map_err(database_error)?;if order!=i.event_order{return Err(invalid.into());}}}
  }
 }
 for table in ["deposits","trades","dividends","movements","openings"]{let mut q=connection.prepare(&format!("SELECT id FROM {table} WHERE source='CSV 导入'")).map_err(database_error)?;for id in q.query_map([],|r|r.get::<_,String>(0)).map_err(database_error)?{if !targets.contains(&(table.to_string(),id.map_err(database_error)?)){return Err(invalid.into());}}}
 let mut orders=HashSet::new();let mut q=connection.prepare("SELECT event_order FROM trades UNION ALL SELECT event_order FROM movements UNION ALL SELECT event_order FROM dividends").map_err(database_error)?;for order in q.query_map([],|r|r.get::<_,i64>(0)).map_err(database_error)?{orders.insert(order.map_err(database_error)?);}
 for i in &archive.items{if matches!(i.current,ImportRecord::Withdrawal(_))&&!orders.insert(i.event_order){return Err(invalid.into());}}
 Ok(())
}
