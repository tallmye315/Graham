use super::{amount_text,database_error,snapshot_from,validate_record_fields,trades,Ledger,Snapshot};
use rusqlite::{params,Connection,OptionalExtension,Transaction,TransactionBehavior};
use serde::{Deserialize,Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct DividendInput {
 pub account_id:String,pub occurred_on:String,pub paid_on:Option<String>,pub status:String,
 pub market:String,pub code:String,pub kind:String,pub name:String,pub currency:String,
 pub gross:String,pub tax:String,pub net:String,pub note:String,
}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Dividend {pub id:String,pub input:DividendInput,pub source:String,pub deleted:bool,pub revision_id:i64,pub event_order:i64}
#[derive(Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct DividendRevision {pub id:i64,pub dividend_id:String,pub input:DividendInput,pub deleted:bool,pub changed_at:String,pub action:String}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct DividendTotal {pub account_id:String,pub market:String,pub code:String,pub name:String,pub kind:String,pub currency:String,pub gross:String,pub tax:String,pub net:String,pub pending:String,pub paid:String,pub record_ids:Vec<String>}

pub(super) fn validate_values(input:&DividendInput)->Result<(),String>{
 validate_record_fields(&input.occurred_on,&input.note)?;
 trades::validate_security(&input.market,&input.code,&input.kind,&input.name,&input.currency)?;
 let gross=trades::scaled(&input.gross,2,false)?;let tax=trades::scaled(&input.tax,2,true)?;let net=trades::scaled(&input.net,2,true)?;
 if tax>gross||gross-tax!=net{return Err("分红净额必须等于税前金额减扣税，扣税不能超过税前金额".into());}
 match(input.status.as_str(),input.paid_on.as_deref()){
  ("pending",None)=>{},
  ("paid",Some(date))=>{validate_record_fields(date,"")?;if date<input.occurred_on.as_str(){return Err("实际到账日期不能早于确认日期".into());}},
  _=>return Err("待到账请留空到账日期；已到账须填写实际到账日期".into()),
 }Ok(())
}
fn validate_account(connection:&Connection,input:&DividendInput,creating:bool)->Result<(),String>{
 let (currency,archived):(String,bool)=connection.query_row("SELECT currency,archived FROM accounts WHERE id=?1",[&input.account_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(database_error)?.ok_or("账户不存在，请刷新")?;
 if currency!=input.currency{return Err("分红原币须与账户及证券市场一致".into());}
 if creating&&archived{return Err("归档账户不能新增分红".into());}Ok(())
}
pub(super) fn load_dividends(connection:&Connection)->Result<Vec<Dividend>,String>{
 let mut q=connection.prepare("SELECT id,data,source,deleted,(SELECT MAX(id) FROM dividend_revisions WHERE dividend_id=dividends.id),event_order FROM dividends ORDER BY event_order").map_err(database_error)?;
 let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?))).map_err(database_error)?.collect::<Result<Vec<_>,_>>().map_err(database_error)?;
 rows.into_iter().map(|(id,data,source,deleted,revision_id,event_order)|Ok(Dividend{id,input:serde_json::from_str(&data).map_err(|_|"分红记录损坏，请保留账本")?,source,deleted,revision_id,event_order})).collect()
}
pub(super) fn load_history(connection:&Connection)->Result<Vec<DividendRevision>,String>{
 let mut q=connection.prepare("SELECT id,dividend_id,data,deleted,changed_at,action FROM dividend_revisions ORDER BY id DESC").map_err(database_error)?;
 let rows=q.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).map_err(database_error)?.collect::<Result<Vec<_>,_>>().map_err(database_error)?;
 rows.into_iter().map(|(id,dividend_id,data,deleted,changed_at,action)|Ok(DividendRevision{id,dividend_id,input:serde_json::from_str(&data).map_err(|_|"分红历史损坏")?,deleted,changed_at,action})).collect()
}
pub(super) fn totals(records:&[Dividend])->Result<Vec<DividendTotal>,String>{
 let mut grouped:BTreeMap<(String,String,String),DividendTotal>=BTreeMap::new();
 for d in records.iter().filter(|d|!d.deleted){let t=&d.input;
  let total=grouped.entry((t.account_id.clone(),t.market.clone(),t.code.clone())).or_insert_with(||DividendTotal{account_id:t.account_id.clone(),market:t.market.clone(),code:t.code.clone(),kind:t.kind.clone(),name:t.name.clone(),currency:t.currency.clone(),gross:"0.00".into(),tax:"0.00".into(),net:"0.00".into(),pending:"0.00".into(),paid:"0.00".into(),record_ids:Vec::new()});
  for (target,value) in [(&mut total.gross,&t.gross),(&mut total.tax,&t.tax),(&mut total.net,&t.net)]{*target=add_money(target,value)?;}
  let target=if t.status=="paid"{&mut total.paid}else{&mut total.pending};*target=add_money(target,&t.net)?;total.record_ids.push(d.id.clone());
 }Ok(grouped.into_values().collect())
}
fn add_money(a:&str,b:&str)->Result<String,String>{Ok(amount_text(trades::scaled(a,2,true)?.checked_add(trades::scaled(b,2,true)?).ok_or("分红汇总超出支持范围")?))}
fn save_revision(tx:&Transaction<'_>,id:&str,action:&str)->Result<(),String>{tx.execute("INSERT INTO dividend_revisions(dividend_id,data,deleted,action) SELECT id,data,deleted,?2 FROM dividends WHERE id=?1",params![id,action]).map_err(database_error)?;Ok(())}
fn finish(tx:Transaction<'_>)->Result<Snapshot,String>{let snapshot=snapshot_from(&tx)?;tx.commit().map_err(database_error)?;Ok(snapshot)}
enum Change<'a>{Revise(&'a DividendInput),State(bool),Restore(i64),Purge}
impl Ledger{
 pub fn record_dividend(&mut self,input:&DividendInput)->Result<Snapshot,String>{
  validate_values(input)?;let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;validate_account(&tx,input,true)?;
  let id=Uuid::new_v4().to_string();let order=super::movements::next_event_order(&tx)?;
  tx.execute("INSERT INTO dividends(id,account_id,data,source,deleted,event_order) VALUES(?1,?2,?3,'手动录入',0,?4)",params![id,input.account_id,serde_json::to_string(input).map_err(|_|"无法保存分红")?,order]).map_err(database_error)?;
  save_revision(&tx,&id,"录入")?;finish(tx)
 }
 pub fn revise_dividend(&mut self,id:&str,expected:i64,input:&DividendInput)->Result<Snapshot,String>{self.change_dividend(id,expected,Change::Revise(input))}
 pub fn set_dividend_deleted(&mut self,id:&str,expected:i64,deleted:bool)->Result<Snapshot,String>{self.change_dividend(id,expected,Change::State(deleted))}
 pub fn restore_dividend_revision(&mut self,id:&str,expected:i64,revision:i64)->Result<Snapshot,String>{self.change_dividend(id,expected,Change::Restore(revision))}
 pub fn permanently_delete_dividend(&mut self,id:&str,expected:i64)->Result<Snapshot,String>{self.change_dividend(id,expected,Change::Purge)}
 fn change_dividend(&mut self,id:&str,expected:i64,change:Change<'_>)->Result<Snapshot,String>{
  let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
  let old=load_dividends(&tx)?.into_iter().find(|d|d.id==id&&d.revision_id==expected).ok_or("分红已变化或不存在，请刷新后重新选择")?;
  if old.source!="手动录入"{return Err("当前操作仅支持手动分红记录".into());}
  let(input,deleted,action)=match change{
   Change::Purge=>{if !old.deleted{return Err("仅已删除分红可以彻底删除".into());}tx.execute("DELETE FROM dividend_revisions WHERE dividend_id=?1",[id]).map_err(database_error)?;tx.execute("DELETE FROM dividends WHERE id=?1",[id]).map_err(database_error)?;return finish(tx);},
   Change::Revise(input)=>{if old.deleted{return Err("请先恢复分红再修改".into());}(input.clone(),false,"修改")},
   Change::State(deleted)=>{if old.deleted==deleted{return Err("分红状态已变化，请刷新".into());}(old.input.clone(),deleted,if deleted{"删除"}else{"恢复"})},
   Change::Restore(revision)=>{let r=load_history(&tx)?.into_iter().find(|r|r.dividend_id==id&&r.id==revision).ok_or("历史版本不存在或已变化，请刷新")?;(r.input,r.deleted,"恢复版本")},
  };
  if input.account_id!=old.input.account_id{return Err("不能更换账户；账户选错请删除后重新录入".into());}
  validate_values(&input)?;validate_account(&tx,&input,false)?;
  tx.execute("UPDATE dividends SET data=?2,deleted=?3 WHERE id=?1",params![id,serde_json::to_string(&input).map_err(|_|"无法保存分红")?,deleted]).map_err(database_error)?;
  save_revision(&tx,id,action)?;finish(tx)
 }
 pub fn dividend_history(&self,id:&str)->Result<Vec<DividendRevision>,String>{Ok(load_history(&self.connection)?.into_iter().filter(|r|r.dividend_id==id).collect())}
}
