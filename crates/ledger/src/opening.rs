use super::{amount_text, database_error, snapshot_from, validate_record_fields, trades, Ledger, Position, Snapshot};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningPosition {
    pub market: String, pub code: String, pub kind: String, pub name: String, pub currency: String,
    pub quantity: String, pub cost: String, pub reference_price: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningInput { pub account_id: String, pub occurred_on: String, pub cash: String, pub positions: Vec<OpeningPosition>, pub note: String }
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Opening { pub id: String, pub input: OpeningInput, pub source: String, pub deleted: bool, pub revision_id: i64 }
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningRevision { pub id: i64, pub opening_id: String, pub input: OpeningInput, pub deleted: bool, pub changed_at: String, pub action: String }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatedAccount { pub receivable: Option<String>, pub has_activity: bool, pub account_id: String, pub name: String, pub currency: String, pub balance: Option<String>, pub positions: Vec<Position>, pub unavailable_reason: Option<String> }
#[derive(Debug, Serialize)]
pub struct DatedSnapshot { pub date: String, pub accounts: Vec<DatedAccount> }

pub(super) fn validate_input(input: &OpeningInput) -> Result<(), String> {
    validate_record_fields(&input.occurred_on, &input.note)?;
    trades::scaled(&input.cash, 2, true)?;
    if input.positions.len() > 1000 { return Err("单个期初快照最多支持 1000 项持仓".into()); }
    let mut ids = HashSet::new();
    for p in &input.positions {
        trades::validate_security(&p.market,&p.code,&p.kind,&p.name,&p.currency)?;
        if !ids.insert((&p.market,&p.code)) { return Err("期初持仓中同一证券不能重复录入".into()); }
        trades::scaled(&p.quantity,6,false)?; trades::scaled(&p.cost,2,true)?;
        if let Some(price) = &p.reference_price { trades::scaled(price,6,false)?; }
    }
    Ok(())
}

pub(super) fn load_openings(connection: &Connection) -> Result<Vec<Opening>, String> {
    let mut q=connection.prepare("SELECT id,data,source,deleted,(SELECT MAX(id) FROM opening_revisions WHERE opening_id=openings.id) FROM openings ORDER BY rowid").map_err(database_error)?;
    let rows=q.query_map([],|r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,i64>(4)?))).map_err(database_error)?.collect::<Result<Vec<_>,_>>().map_err(database_error)?;
    rows.into_iter().map(|(id,data,source,deleted,revision_id)|Ok(Opening{id,input:serde_json::from_str(&data).map_err(|_|"期初记录损坏，请保留账本")?,source,deleted,revision_id})).collect()
}

pub(super) fn load_history(connection: &Connection) -> Result<Vec<OpeningRevision>, String> {
    let mut q=connection.prepare("SELECT id,opening_id,data,deleted,changed_at,action FROM opening_revisions ORDER BY id DESC").map_err(database_error)?;
    let rows=q.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).map_err(database_error)?.collect::<Result<Vec<_>,_>>().map_err(database_error)?;
    rows.into_iter().map(|(id,opening_id,data,deleted,changed_at,action)|Ok(OpeningRevision{id,opening_id,input:serde_json::from_str(&data).map_err(|_|"期初历史损坏")?,deleted,changed_at,action})).collect()
}

fn write_state(tx: &Transaction<'_>, id: &str, input: &OpeningInput, deleted: bool, action: &str) -> Result<(),String> {
    super::imports::ensure_manual(tx,"openings",id)?;
    validate_input(input)?;
    let currency:String=tx.query_row("SELECT currency FROM accounts WHERE id=?1",[&input.account_id],|r|r.get(0)).optional().map_err(database_error)?.ok_or("投资账户不存在")?;
    if input.positions.iter().any(|p|p.currency!=currency) { return Err("期初持仓原币必须与账户币种一致，跨币种结算留待换汇功能".into()); }
    let data=serde_json::to_string(input).map_err(|_|"无法保存期初快照")?;
    tx.execute("UPDATE openings SET data=?2,deleted=?3 WHERE id=?1",params![id,data,deleted]).map_err(database_error)?;
    tx.execute("INSERT INTO opening_revisions(opening_id,data,deleted,action) VALUES(?1,?2,?3,?4)",params![id,data,deleted,action]).map_err(database_error)?;
    Ok(())
}
fn finish(tx: Transaction<'_>) -> Result<Snapshot,String> {
    let snapshot=snapshot_from(&tx)?; tx.commit().map_err(database_error)?; Ok(snapshot)
}
fn selected(tx: &Transaction<'_>,id:&str,expected:i64)->Result<Opening,String>{
    load_openings(tx)?.into_iter().find(|o|o.id==id && o.revision_id==expected).ok_or_else(||"期初快照已变化或不存在，请刷新后重新选择".into())
}

impl Ledger {
    pub fn save_opening(&mut self,input:&OpeningInput,expected_revision:Option<i64>)->Result<Snapshot,String>{
        validate_input(input)?;
        let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let existing=load_openings(&tx)?.into_iter().find(|o|o.input.account_id==input.account_id);
        let (id,action)=match (existing,expected_revision){
            (Some(old),Some(expected)) if old.revision_id==expected && !old.deleted => (old.id,"修改"),
            (None,None)=>{
                let active:bool=tx.query_row("SELECT archived=0 FROM accounts WHERE id=?1",[&input.account_id],|r|r.get(0)).optional().map_err(database_error)?.ok_or("账户不存在")?;
                if !active{return Err("归档账户不能新增期初快照".into());}
                let id=Uuid::new_v4().to_string();
                tx.execute("INSERT INTO openings(id,account_id,data,source,deleted) VALUES(?1,?2,'{}','手动核对',0)",params![id,input.account_id]).map_err(database_error)?;
                (id,"建立")
            },
            _=>return Err("该账户期初快照已变化，请刷新；已删除快照须先恢复或彻底删除".into()),
        };
        write_state(&tx,&id,input,false,action)?; finish(tx)
    }
    pub fn set_opening_deleted(&mut self,id:&str,expected:i64,deleted:bool)->Result<Snapshot,String>{
        let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let old=selected(&tx,id,expected)?;
        if old.deleted==deleted{return Err("期初快照状态已变化，请刷新".into());}
        write_state(&tx,id,&old.input,deleted,if deleted{"删除"}else{"恢复"})?;finish(tx)
    }
    pub fn restore_opening_revision(&mut self,id:&str,expected:i64,revision:i64)->Result<Snapshot,String>{
        let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let old=selected(&tx,id,expected)?;
        let r=load_history(&tx)?.into_iter().find(|r|r.opening_id==id && r.id==revision).ok_or("历史版本已变化或不存在，请刷新")?;
        if old.input.account_id!=r.input.account_id{return Err("历史版本账户不一致".into());}
        write_state(&tx,id,&r.input,r.deleted,"恢复版本")?;finish(tx)
    }
    pub fn permanently_delete_opening(&mut self,id:&str,expected:i64)->Result<Snapshot,String>{
        let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        super::imports::ensure_manual(&tx,"openings",id)?;
        let old=selected(&tx,id,expected)?;
        if !old.deleted{return Err("仅已删除期初快照可以彻底删除".into());}
        tx.execute("DELETE FROM opening_revisions WHERE opening_id=?1",[id]).map_err(database_error)?;
        tx.execute("DELETE FROM openings WHERE id=?1",[id]).map_err(database_error)?;finish(tx)
    }
    pub fn opening_history(&self,id:&str)->Result<Vec<OpeningRevision>,String>{Ok(load_history(&self.connection)?.into_iter().filter(|r|r.opening_id==id).collect())}
    pub fn snapshot_on(&self,date:&str)->Result<DatedSnapshot,String>{
        validate_record_fields(date,"")?;
        let tx=self.connection.unchecked_transaction().map_err(database_error)?;
        let snapshot=snapshot_from(&tx)?;
        let withdrawals=super::imports::withdrawals(&tx)?;
        let calculated=trades::calculate_at(&tx,&snapshot.trades,Some(date))?;
        let mut accounts=Vec::new();
        for a in snapshot.accounts{
            let opening=snapshot.openings.iter().find(|o|!o.deleted && o.input.account_id==a.id);
            let earliest=snapshot.entries.iter().filter(|e|!e.deleted && e.account_id==a.id).map(|e|e.occurred_on.as_str()).chain(snapshot.trades.iter().filter(|t|!t.deleted && t.input.account_id==a.id).map(|t|t.input.occurred_on.as_str())).chain(snapshot.movements.iter().filter(|m|!m.deleted&&(m.input.from_account_id==a.id||m.input.to_account_id==a.id)).map(|m|m.input.occurred_on.as_str())).chain(snapshot.dividends.iter().filter(|d|!d.deleted&&d.input.account_id==a.id).map(|d|d.input.occurred_on.as_str())).chain(withdrawals.iter().filter(|w|w.0==a.id).map(|w|w.1.as_str())).min();
            let reason=if let Some(o)=opening{if date<o.input.occurred_on.as_str(){Some(format!("{} 收盘期初之前数据不足",o.input.occurred_on))}else{None}}else if earliest.is_none_or(|first|date<first){Some("该日尚无可核对起点，数据不足".into())}else{None};
            let positions=if reason.is_some(){Vec::new()}else{calculated.positions.iter().filter(|p|p.account_id==a.id).cloned().collect()};
            let balance=if reason.is_some(){None}else{Some(amount_text(*calculated.balances.get(&a.id).unwrap_or(&0)))};
            let receivable=if reason.is_some(){None}else{snapshot.dividends.iter().filter(|d|!d.deleted&&d.input.account_id==a.id&&d.input.occurred_on.as_str()<=date&&(d.input.status=="pending"||d.input.paid_on.as_deref().is_some_and(|paid|paid>date))).try_fold(0i64,|total,d|trades::scaled(&d.input.net,2,true).ok().and_then(|v|total.checked_add(v))).map(amount_text)};
            accounts.push(DatedAccount{receivable,has_activity:earliest.is_some()||opening.is_some(),account_id:a.id,name:a.name,currency:a.currency,balance,positions,unavailable_reason:reason});
        }
        tx.commit().map_err(database_error)?;Ok(DatedSnapshot{date:date.into(),accounts})
    }
}
