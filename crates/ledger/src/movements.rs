use super::{database_error,snapshot_from,validate_record_fields,trades,Ledger,Snapshot};
use rusqlite::{params,Connection,OptionalExtension,Transaction,TransactionBehavior};
use serde::{Deserialize,Serialize};
use std::collections::{BTreeMap,HashSet};
use uuid::Uuid;

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct MovementInput{
 pub from_account_id:String,pub to_account_id:String,pub occurred_on:String,pub kind:String,
 pub from_currency:String,pub to_currency:String,pub from_amount:String,pub to_amount:String,
 pub actual_rate:String,pub fee:String,pub fee_side:String,pub note:String,
}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Movement{pub id:String,pub input:MovementInput,pub source:String,pub deleted:bool,pub revision_id:i64,pub event_order:i64}
#[derive(Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct MovementRevision{pub id:i64,pub movement_id:String,pub input:MovementInput,pub deleted:bool,pub changed_at:String,pub action:String}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct CurrencyFlow{pub currency:String,pub inflow:String,pub outflow:String,pub net_capital:String,pub fees:String}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct FlowSummary{pub start_on:String,pub end_on:String,pub totals:Vec<CurrencyFlow>,pub internal_count:usize,pub unavailable_reason:Option<String>}

pub(super) fn validate_values(input:&MovementInput)->Result<(),String>{
 validate_record_fields(&input.occurred_on,&input.note)?;
 if input.from_account_id==input.to_account_id{return Err("请选择两个不同的账户；同一券商不同币种请分别建账".into());}
 if !["CNY","USD","HKD"].contains(&input.from_currency.as_str())||!["CNY","USD","HKD"].contains(&input.to_currency.as_str()){return Err("不支持该币种".into());}
 if !["from","to"].contains(&input.fee_side.as_str()){return Err("请选择手续费扣款账户".into());}
 let from=trades::scaled(&input.from_amount,2,false)?;let to=trades::scaled(&input.to_amount,2,false)?;let rate=trades::scaled(&input.actual_rate,8,false)?;
 match input.kind.as_str(){
  "transfer" if input.from_currency==input.to_currency&&from==to&&rate==100_000_000=>{},
  "exchange" if input.from_currency!=input.to_currency=>{let converted=(from as i128*rate as i128+50_000_000)/100_000_000;if converted!=to as i128{return Err("实际汇率与两端本金不符：汇率为 1 转出币对应的转入币，折算四舍五入到分".into());}},
  _=>return Err("同币种转账本金须相等、汇率为 1；不同币种请选择换汇".into()),
 }
 cash_deltas(input)?;Ok(())
}
fn validate_accounts(connection:&Connection,input:&MovementInput)->Result<(),String>{
 for (id,currency) in [(&input.from_account_id,&input.from_currency),(&input.to_account_id,&input.to_currency)]{
  let stored:String=connection.query_row("SELECT currency FROM accounts WHERE id=?1",[id],|r|r.get(0)).optional().map_err(database_error)?.ok_or("转出或转入账户不存在，请刷新")?;
  if &stored!=currency{return Err("记录原币与账户币种不一致，请刷新账户".into());}
 }Ok(())
}
pub(super) fn cash_deltas(input:&MovementInput)->Result<(i64,i64),String>{
 let from=trades::scaled(&input.from_amount,2,false)?;let to=trades::scaled(&input.to_amount,2,false)?;let fee=trades::scaled(&input.fee,2,true)?;
 let debit=from.checked_add(if input.fee_side=="from"{fee}else{0}).and_then(|n|n.checked_neg()).ok_or("转出本金加手续费超出支持范围")?;
 let credit=to.checked_sub(if input.fee_side=="to"{fee}else{0}).filter(|n|*n>=0).ok_or("转入端手续费不能超过转入本金")?;
 Ok((debit,credit))
}
pub(super) fn next_event_order(connection:&Connection)->Result<i64,String>{
 let latest:i64=connection.query_row("SELECT COALESCE(MAX(event_order),0) FROM (SELECT event_order FROM trades UNION ALL SELECT event_order FROM movements UNION ALL SELECT event_order FROM dividends UNION ALL SELECT CAST(json_extract(data,'$.eventOrder') AS INTEGER) FROM import_items WHERE record_type='withdrawal')",[],|r|r.get(0)).map_err(database_error)?;
 latest.checked_add(1).ok_or_else(||"记录顺序超出支持范围".into())
}
pub(super) fn load_movements(connection:&Connection)->Result<Vec<Movement>,String>{
 let mut q=connection.prepare("SELECT id,data,source,deleted,(SELECT MAX(id) FROM movement_revisions WHERE movement_id=movements.id),event_order FROM movements ORDER BY event_order").map_err(database_error)?;
 let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?))).map_err(database_error)?.collect::<Result<Vec<_>,_>>().map_err(database_error)?;
 rows.into_iter().map(|(id,data,source,deleted,revision_id,event_order)|Ok(Movement{id,input:serde_json::from_str(&data).map_err(|_|"关联资金记录损坏，请保留账本")?,source,deleted,revision_id,event_order})).collect()
}
pub(super) fn load_history(connection:&Connection)->Result<Vec<MovementRevision>,String>{
 let mut q=connection.prepare("SELECT id,movement_id,data,deleted,changed_at,action FROM movement_revisions ORDER BY id DESC").map_err(database_error)?;
 let rows=q.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).map_err(database_error)?.collect::<Result<Vec<_>,_>>().map_err(database_error)?;
 rows.into_iter().map(|(id,movement_id,data,deleted,changed_at,action)|Ok(MovementRevision{id,movement_id,input:serde_json::from_str(&data).map_err(|_|"关联资金历史损坏")?,deleted,changed_at,action})).collect()
}
fn save_revision(tx:&Transaction<'_>,id:&str,action:&str)->Result<(),String>{
 tx.execute("INSERT INTO movement_revisions(movement_id,data,deleted,action) SELECT id,data,deleted,?2 FROM movements WHERE id=?1",params![id,action]).map_err(database_error)?;Ok(())
}
fn finish(tx:Transaction<'_>)->Result<Snapshot,String>{let snapshot=snapshot_from(&tx)?;tx.commit().map_err(database_error)?;Ok(snapshot)}
enum Change<'a>{Revise(&'a MovementInput),State(bool),Restore(i64),Purge}

impl Ledger{
 pub fn record_movement(&mut self,input:&MovementInput)->Result<Snapshot,String>{
  validate_values(input)?;let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;validate_accounts(&tx,input)?;
  for id in [&input.from_account_id,&input.to_account_id]{let archived:bool=tx.query_row("SELECT archived FROM accounts WHERE id=?1",[id],|r|r.get(0)).map_err(database_error)?;if archived{return Err("归档账户不能新增转账或换汇".into());}}
  let id=Uuid::new_v4().to_string();let order=next_event_order(&tx)?;
  tx.execute("INSERT INTO movements(id,from_account_id,to_account_id,data,source,deleted,event_order) VALUES(?1,?2,?3,?4,'手动录入',0,?5)",params![id,input.from_account_id,input.to_account_id,serde_json::to_string(input).map_err(|_|"无法保存关联记录")?,order]).map_err(database_error)?;
  save_revision(&tx,&id,"录入")?;finish(tx)
 }
 pub fn revise_movement(&mut self,id:&str,expected:i64,input:&MovementInput)->Result<Snapshot,String>{self.change_movement(id,expected,Change::Revise(input))}
 pub fn set_movement_deleted(&mut self,id:&str,expected:i64,deleted:bool)->Result<Snapshot,String>{self.change_movement(id,expected,Change::State(deleted))}
 pub fn restore_movement_revision(&mut self,id:&str,expected:i64,revision:i64)->Result<Snapshot,String>{self.change_movement(id,expected,Change::Restore(revision))}
 pub fn permanently_delete_movement(&mut self,id:&str,expected:i64)->Result<Snapshot,String>{self.change_movement(id,expected,Change::Purge)}
 fn change_movement(&mut self,id:&str,expected:i64,change:Change<'_>)->Result<Snapshot,String>{
  let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
  let old=load_movements(&tx)?.into_iter().find(|m|m.id==id&&m.revision_id==expected).ok_or("关联记录已变化或不存在，请刷新后重新选择")?;
  if old.source!="手动录入"{return Err("当前操作仅支持手动关联资金记录".into());}
  let (input,deleted,action)=match change{
   Change::Purge=>{if !old.deleted{return Err("仅已删除关联记录可以彻底删除".into());}tx.execute("DELETE FROM movement_revisions WHERE movement_id=?1",[id]).map_err(database_error)?;tx.execute("DELETE FROM movements WHERE id=?1",[id]).map_err(database_error)?;return finish(tx);},
   Change::Revise(input)=>{if old.deleted{return Err("请先恢复记录再修改".into());}(input.clone(),false,"修改")},
   Change::State(deleted)=>{if old.deleted==deleted{return Err("记录状态已变化，请刷新".into());}(old.input.clone(),deleted,if deleted{"删除"}else{"恢复"})},
   Change::Restore(revision)=>{let r=load_history(&tx)?.into_iter().find(|r|r.movement_id==id&&r.id==revision).ok_or("历史版本不存在或已变化，请刷新")?;(r.input,r.deleted,"恢复版本")},
  };
  if input.from_account_id!=old.input.from_account_id||input.to_account_id!=old.input.to_account_id{return Err("修改不能更换两端账户；账户选错请整笔删除后重新录入".into());}
  validate_values(&input)?;validate_accounts(&tx,&input)?;
  tx.execute("UPDATE movements SET data=?2,deleted=?3 WHERE id=?1",params![id,serde_json::to_string(&input).map_err(|_|"无法保存关联记录")?,deleted]).map_err(database_error)?;
  save_revision(&tx,id,action)?;finish(tx)
 }
 pub fn movement_history(&self,id:&str)->Result<Vec<MovementRevision>,String>{Ok(load_history(&self.connection)?.into_iter().filter(|r|r.movement_id==id).collect())}
 pub fn cash_flows(&self,account_ids:&[String],start_on:&str,end_on:&str)->Result<FlowSummary,String>{
  validate_record_fields(start_on,"")?;validate_record_fields(end_on,"")?;if start_on>end_on{return Err("开始日期不能晚于结束日期".into());}
  let ids:HashSet<_>=account_ids.iter().collect();if ids.is_empty()||ids.len()!=account_ids.len(){return Err("请选择账户，同一账户不能重复".into());}
  let tx=self.connection.unchecked_transaction().map_err(database_error)?;
  let snapshot=snapshot_from(&tx)?;if account_ids.iter().any(|id|!snapshot.accounts.iter().any(|a|&a.id==id)){return Err("账户已变化，请刷新".into());}
  if let Some(o)=snapshot.openings.iter().find(|o|!o.deleted&&ids.contains(&o.input.account_id)&&start_on<=o.input.occurred_on.as_str()){
   return Ok(FlowSummary{start_on:start_on.into(),end_on:end_on.into(),totals:Vec::new(),internal_count:0,unavailable_reason:Some(format!("查询范围包含 {} 收盘期初或此前，数据不足；请从该账户期初次日起核对",o.input.occurred_on))});
  }
  let mut totals:BTreeMap<String,(i128,i128,i128)>=snapshot.accounts.iter().filter(|a|ids.contains(&a.id)).map(|a|(a.currency.clone(),(0,0,0))).collect();
  for e in &snapshot.entries{if !e.deleted&&ids.contains(&e.account_id)&&e.occurred_on.as_str()>=start_on&&e.occurred_on.as_str()<=end_on{let a=snapshot.accounts.iter().find(|a|a.id==e.account_id).ok_or("入金账户不存在")?;totals.get_mut(&a.currency).ok_or("币种不存在")?.0+=trades::scaled(&e.amount,2,true)? as i128;}}
  for(account,date,delta,_)in super::imports::withdrawals(&tx)?{if account_ids.contains(&account)&&date.as_str()>=start_on&&date.as_str()<=end_on{let a=snapshot.accounts.iter().find(|a|a.id==account).ok_or("出金账户不存在")?;totals.get_mut(&a.currency).ok_or("出金币种不存在")?.1+=-(delta as i128);}}
  let mut internal_count=0;
  for m in snapshot.movements.iter().filter(|m|!m.deleted&&m.input.occurred_on.as_str()>=start_on&&m.input.occurred_on.as_str()<=end_on){
   let t=&m.input;let from=ids.contains(&t.from_account_id);let to=ids.contains(&t.to_account_id);
   if from&&to{internal_count+=1;}else if from{totals.get_mut(&t.from_currency).ok_or("币种不存在")?.1+=trades::scaled(&t.from_amount,2,false)? as i128;}else if to{totals.get_mut(&t.to_currency).ok_or("币种不存在")?.0+=trades::scaled(&t.to_amount,2,false)? as i128;}
   let fee_currency=if t.fee_side=="from"&&from{Some(&t.from_currency)}else if t.fee_side=="to"&&to{Some(&t.to_currency)}else{None};
   if let Some(currency)=fee_currency{totals.get_mut(currency).ok_or("币种不存在")?.2+=trades::scaled(&t.fee,2,true)? as i128;}
  }
  let totals=totals.into_iter().map(|(currency,(incoming,outgoing,fees))|CurrencyFlow{currency,inflow:money(incoming),outflow:money(outgoing),net_capital:money(incoming-outgoing),fees:money(fees)}).collect();
  tx.commit().map_err(database_error)?;
  Ok(FlowSummary{start_on:start_on.into(),end_on:end_on.into(),totals,internal_count,unavailable_reason:None})
 }
}
fn money(cents:i128)->String{let value=cents.unsigned_abs();format!("{}{}.{:02}",if cents<0{"-"}else{""},value/100,value%100)}
