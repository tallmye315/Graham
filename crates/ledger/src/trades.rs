use super::{amount_text, database_error, snapshot_from, validate_record_fields, Ledger, Snapshot};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

const SCALE: i64 = 1_000_000;
const RANGE: &str = "交易数量或金额超出支持范围";

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TradeInput {
    pub account_id: String,
    pub occurred_on: String,
    pub market: String,
    pub code: String,
    pub kind: String,
    pub name: String,
    pub currency: String,
    pub side: String,
    pub quantity: String,
    pub price: String,
    pub fee: String,
    pub tax: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Trade {
    pub id: String,
    pub input: TradeInput,
    pub source: String,
    pub deleted: bool,
    pub revision_id: i64,
    #[serde(default)]
    pub event_order: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub account_id: String,
    pub market: String,
    pub code: String,
    pub kind: String,
    pub name: String,
    pub currency: String,
    pub quantity: String,
    pub cost: String,
    pub average_cost: String,
    pub realized: String,
    pub since: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionValue { pub market_value: String, pub unrealized: String }

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TradeRevision {
    pub id: i64,
    pub trade_id: String,
    pub input: TradeInput,
    pub deleted: bool,
    pub changed_at: String,
    pub action: String,
}

pub(super) fn scaled(text: &str, digits: u32, allow_zero: bool) -> Result<i64, String> {
    let text = text.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) || fraction.len() > digits as usize || (text.contains('.') && fraction.is_empty()) {
        return Err(format!("请输入{}数，最多 {digits} 位小数", if allow_zero { "非负" } else { "正" }));
    }
    let multiplier = 10i64.pow(digits);
    let fraction = format!("{fraction:0<width$}", width = digits as usize).parse::<i64>().map_err(|_| RANGE)?;
    whole.parse::<i64>().ok().and_then(|v| v.checked_mul(multiplier)).and_then(|v| v.checked_add(fraction)).filter(|v| *v > 0 || (allow_zero && *v == 0)).ok_or_else(|| RANGE.into())
}

fn rounded(numerator: i128, denominator: i128) -> Result<i64, String> {
    i64::try_from((numerator + denominator / 2) / denominator).map_err(|_| RANGE.into())
}

fn decimal(value: i64) -> String {
    format!("{}.{:06}", value / SCALE, value % SCALE).trim_end_matches('0').trim_end_matches('.').to_string()
}

pub(super) fn validate_input(input: &TradeInput) -> Result<(), String> {
    validate_record_fields(&input.occurred_on, &input.note)?;
    validate_security(&input.market,&input.code,&input.kind,&input.name,&input.currency)?;
    if !["buy","sell"].contains(&input.side.as_str()){return Err("请选择买入或卖出".into());}
    trade_amounts(input)?;
    Ok(())
}

pub(super) fn validate_security(market:&str,code:&str,kind:&str,name:&str,currency:&str)->Result<(),String>{
    let valid_code = match market {
        "CN" => code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit()),
        "HK" => code.len() == 5 && code.bytes().all(|b| b.is_ascii_digit()),
        "US" => !code.is_empty() && code.len() <= 20 && code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'.' || b == b'-'),
        _ => false,
    };
    if !valid_code { return Err("请使用 A 股六位代码、港股五位代码或大写美股代码，并选择对应市场".into()); }
    if !["stock", "etf"].contains(&kind) { return Err("请选择股票或 ETF，以及买入或卖出".into()); }
    if name.trim().is_empty() || name.chars().count() > 80 { return Err("证券名称应为 1—80 个字符".into()); }
    let expected_currency = match market { "CN" => "CNY", "HK" => "HKD", _ => "USD" };
    if currency != expected_currency { return Err("证券原币须与市场一致；跨币种结算将在换汇功能中支持".into()); }
    Ok(())
}

fn trade_amounts(input: &TradeInput) -> Result<(i64, i64, i64), String> {
    let quantity = scaled(&input.quantity, 6, false)?;
    let price = scaled(&input.price, 6, false)?;
    let gross = rounded(quantity as i128 * price as i128, 10_000_000_000)?;
    if gross == 0 { return Err("成交金额四舍五入到分后必须大于零".into()); }
    let charges = scaled(&input.fee, 2, true)?.checked_add(scaled(&input.tax, 2, true)?).ok_or(RANGE)?;
    let cash = if input.side == "buy" { gross.checked_add(charges).and_then(|v| v.checked_neg()).ok_or(RANGE)? } else { gross.checked_sub(charges).ok_or(RANGE)? };
    Ok((quantity, gross, cash))
}

pub(super) fn load_trades(connection: &Connection) -> Result<Vec<Trade>, String> {
    let mut query = connection.prepare("SELECT id,data,source,deleted,(SELECT MAX(id) FROM trade_revisions WHERE trade_id=trades.id),event_order FROM trades ORDER BY event_order").map_err(database_error)?;
    let rows = query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, bool>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?))).map_err(database_error)?.collect::<Result<Vec<_>, _>>().map_err(database_error)?;
    rows.into_iter().map(|(id, data, source, deleted, revision_id, event_order)| Ok(Trade { id, input: serde_json::from_str(&data).map_err(|_| "交易记录损坏，请保留账本")?, source, deleted, revision_id, event_order })).collect()
}

#[derive(Default)]
struct Holding { quantity: i64, cost: i64, realized: i64, kind: String, name: String, currency: String }

pub(super) struct Calculated { pub balances: BTreeMap<String, i64>, pub positions: Vec<Position> }

pub(super) fn calculate(connection: &Connection, trades: &[Trade]) -> Result<Calculated, String> { calculate_at(connection,trades,None) }

pub(super) fn calculate_at(connection: &Connection, trades: &[Trade], as_of: Option<&str>) -> Result<Calculated, String> {
    let openings=super::opening::load_openings(connection)?;
    let active: BTreeMap<_,_>=openings.iter().filter(|o|!o.deleted).map(|o|(o.input.account_id.as_str(),o)).collect();
    let included=|account:&str,date:&str| as_of.is_none_or(|end|date<=end) && active.get(account).is_none_or(|o|date>o.input.occurred_on.as_str());
    // Date ordering is stable: deposits precede trades on the same day; trades keep creation order.
    let mut events: Vec<(String, u8, i64, String, i64)> = Vec::new();
    let mut deposits = connection.prepare("SELECT account_id,occurred_on,amount_cents FROM deposits WHERE deleted=0 ORDER BY rowid").map_err(database_error)?;
    for (index, row) in deposits.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))).map_err(database_error)?.enumerate() {
        let (account, date, cents) = row.map_err(database_error)?;
        if included(&account,&date) { events.push((date, 0, index as i64, account, cents)); }
    }
    let mut security_kinds = BTreeMap::new();
    for trade in trades.iter().filter(|t| !t.deleted && included(&t.input.account_id,&t.input.occurred_on)) {
        let key = (&trade.input.market, &trade.input.code);
        if let Some(kind) = security_kinds.insert(key, &trade.input.kind) {
            if kind != &trade.input.kind { return Err("同一市场和代码的证券类型冲突，请先核对或修正相关交易".into()); }
        }
    }
    let mut holdings: BTreeMap<(String, String, String), Holding> = BTreeMap::new();
    for o in active.values().filter(|o|as_of.is_none_or(|end|o.input.occurred_on.as_str()<=end)) {
        events.push((o.input.occurred_on.clone(),0,0,o.input.account_id.clone(),scaled(&o.input.cash,2,true)?));
        for p in &o.input.positions {
            if let Some(kind)=security_kinds.insert((&p.market,&p.code),&p.kind){if kind!=&p.kind{return Err("期初与交易的证券类型冲突".into());}}
            holdings.insert((o.input.account_id.clone(),p.market.clone(),p.code.clone()),Holding{quantity:scaled(&p.quantity,6,false)?,cost:scaled(&p.cost,2,true)?,realized:0,kind:p.kind.clone(),name:p.name.clone(),currency:p.currency.clone()});
        }
    }
    let mut ordered: Vec<_> = trades.iter().enumerate().filter(|(_, t)| !t.deleted && included(&t.input.account_id,&t.input.occurred_on)).collect();
    ordered.sort_by(|(_,a), (_,b)| a.input.occurred_on.cmp(&b.input.occurred_on).then(a.event_order.cmp(&b.event_order)));
    for (_, trade) in ordered {
        let t = &trade.input;
        let (quantity, _, cash) = trade_amounts(t)?;
        let holding = holdings.entry((t.account_id.clone(), t.market.clone(), t.code.clone())).or_default();
        if !holding.kind.is_empty() && holding.kind != t.kind { return Err(format!("{} {} 的证券类型与已有记录冲突", t.market, t.code)); }
        holding.kind = t.kind.clone(); holding.name = t.name.clone(); holding.currency = t.currency.clone();
        if t.side == "buy" {
            holding.quantity = holding.quantity.checked_add(quantity).ok_or(RANGE)?;
            holding.cost = holding.cost.checked_add(-cash).ok_or(RANGE)?;
        } else {
            if quantity > holding.quantity { return Err(format!("{} {} 在 {} 超卖；请先核对买入和日期，当前账本未修改", t.market, t.code, t.occurred_on)); }
            let released = if quantity == holding.quantity { holding.cost } else { rounded(holding.cost as i128 * quantity as i128, holding.quantity as i128)? };
            holding.quantity -= quantity; holding.cost -= released;
            holding.realized = holding.realized.checked_add(cash.checked_sub(released).ok_or(RANGE)?).ok_or(RANGE)?;
        }
        events.push((t.occurred_on.clone(), 1, trade.event_order, t.account_id.clone(), cash));
    }
    for m in super::movements::load_movements(connection)?.into_iter().filter(|m|!m.deleted){
        let t=&m.input;let (debit,credit)=super::movements::cash_deltas(t)?;
        if included(&t.from_account_id,&t.occurred_on){events.push((t.occurred_on.clone(),1,m.event_order,t.from_account_id.clone(),debit));}
        if included(&t.to_account_id,&t.occurred_on){events.push((t.occurred_on.clone(),1,m.event_order,t.to_account_id.clone(),credit));}
    }
    let dividends=super::dividends::load_dividends(connection)?;
    for d in dividends.iter().filter(|d|!d.deleted){
        let t=&d.input;
        if let Some(kind)=security_kinds.insert((&t.market,&t.code),&t.kind){if kind!=&t.kind{return Err("分红与已有证券类型冲突，请核对".into());}}
        if let Some(date)=t.paid_on.as_deref().filter(|_|t.status=="paid") {
            if included(&t.account_id,date){events.push((date.into(),1,d.event_order,t.account_id.clone(),scaled(&t.net,2,true)?));}
        }
    }
    for(account,date,cash,order)in super::imports::withdrawals(connection)?{if included(&account,&date){events.push((date,1,order,account,cash));}}
    events.sort_by(|a,b| (&a.0,a.1,a.2).cmp(&(&b.0,b.1,b.2)));
    let mut balances: BTreeMap<String, i64> = BTreeMap::new();
    for (date, _, _, account, cash) in events {
        let balance = balances.entry(account).or_default();
        *balance = balance.checked_add(cash).ok_or("账户现金超出支持范围")?;
        if *balance < 0 { return Err(format!("{date} 的账户现金不足，请先核对入金、日期和税费；当前账本未修改")); }
    }
    let positions = holdings.into_iter().map(|((account_id, market, code), h)| Ok(Position {
        since:active.get(account_id.as_str()).map(|o|o.input.occurred_on.clone()),
        account_id, market, code, kind: h.kind, name: h.name, currency: h.currency,
        quantity: decimal(h.quantity), cost: amount_text(h.cost), realized: amount_text(h.realized),
        average_cost: if h.quantity == 0 { "0".into() } else { decimal(rounded(h.cost as i128 * 10_000_000_000, h.quantity as i128)?) },
    })).collect::<Result<Vec<_>, String>>()?;
    Ok(Calculated { balances, positions })
}

pub(super) fn validate_ledger(connection: &Connection) -> Result<(), String> {
    calculate(connection, &load_trades(connection)?)?;
    Ok(())
}

fn save_revision(connection: &Connection, id: &str, action: &str) -> Result<(), String> {
    connection.execute("INSERT INTO trade_revisions(trade_id,data,deleted,action) SELECT id,data,deleted,?2 FROM trades WHERE id=?1", params![id,action]).map_err(database_error)?;
    Ok(())
}

impl Ledger {
    pub fn record_trade(&mut self, input: &TradeInput) -> Result<Snapshot, String> {
        validate_input(input)?;
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let account_currency: String = transaction.query_row("SELECT currency FROM accounts WHERE id=?1 AND archived=0", [&input.account_id], |r| r.get(0)).optional().map_err(database_error)?.ok_or("账户不存在或已归档")?;
        if input.currency != account_currency { return Err("请选择与证券原币一致的账户；暂不支持跨币种结算".into()); }
        let id = Uuid::new_v4().to_string();
        let event_order=super::movements::next_event_order(&transaction)?;
        let data = serde_json::to_string(input).map_err(|_| "无法保存交易")?;
        transaction.execute("INSERT INTO trades(id,account_id,data,source,deleted,event_order) VALUES(?1,?2,?3,'手动录入',0,?4)", params![id,input.account_id,data,event_order]).map_err(database_error)?;
        save_revision(&transaction, &id, "录入")?;
        let snapshot = snapshot_from(&transaction)?;
        transaction.commit().map_err(database_error)?;
        Ok(snapshot)
    }

    pub fn revise_trade(&mut self, id: &str, expected_revision: i64, input: &TradeInput) -> Result<Snapshot, String> {
        validate_input(input)?;
        self.change_trade(id, expected_revision, Some(input), None, None)
    }

    pub fn set_trade_deleted(&mut self, id: &str, expected_revision: i64, deleted: bool) -> Result<Snapshot, String> {
        self.change_trade(id, expected_revision, None, Some(deleted), None)
    }

    pub fn permanently_delete_trade(&mut self, id: &str, expected_revision: i64) -> Result<Snapshot, String> {
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let trade = load_trades(&transaction)?.into_iter().find(|t| t.id == id && t.revision_id == expected_revision).ok_or("交易已变化或不存在，请刷新后重新选择")?;
        if !trade.deleted { return Err("仅已删除交易可以彻底删除，请先普通删除".into()); }
        if trade.source != "手动录入" { return Err("本次彻底删除仅支持手动买卖记录".into()); }
        transaction.execute("DELETE FROM trade_revisions WHERE trade_id=?1", [id]).map_err(database_error)?;
        transaction.execute("DELETE FROM trades WHERE id=?1", [id]).map_err(database_error)?;
        let snapshot = snapshot_from(&transaction)?;
        transaction.commit().map_err(database_error)?;
        Ok(snapshot)
    }

    pub fn restore_trade_revision(&mut self, id: &str, expected_revision: i64, restore_revision: i64) -> Result<Snapshot, String> {
        self.change_trade(id, expected_revision, None, None, Some(restore_revision))
    }

    fn change_trade(&mut self, id: &str, expected_revision: i64, input: Option<&TradeInput>, deleted: Option<bool>, restore: Option<i64>) -> Result<Snapshot, String> {
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        super::imports::ensure_manual(&transaction,"trades",id)?;
        let old = load_trades(&transaction)?.into_iter().find(|t| t.id == id && t.revision_id == expected_revision).ok_or("交易已变化或不存在，请刷新后重试")?;
        let (next, deleted, action) = if let Some(revision) = restore {
            let (data, deleted): (String,bool) = transaction.query_row("SELECT data,deleted FROM trade_revisions WHERE id=?1 AND trade_id=?2", params![revision,id], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(database_error)?.ok_or("历史版本已变化或不存在，请刷新后重试")?;
            (serde_json::from_str::<TradeInput>(&data).map_err(|_| "交易历史损坏")?, deleted, "恢复版本")
        } else if let Some(deleted) = deleted {
            if deleted == old.deleted { return Err("交易状态已变化，请刷新后重试".into()); }
            (old.input.clone(), deleted, if deleted { "删除" } else { "恢复记录" })
        } else {
            if old.deleted { return Err("请先恢复交易再修改".into()); }
            (input.ok_or("缺少交易内容")?.clone(), false, "修改")
        };
        if next.account_id != old.input.account_id { return Err("交易不能改到账户外，请删除后在正确账户重新录入".into()); }
        validate_input(&next)?;
        let currency: String = transaction.query_row("SELECT currency FROM accounts WHERE id=?1", [&next.account_id], |r| r.get(0)).map_err(database_error)?;
        if currency != next.currency { return Err("交易原币与账户币种不一致".into()); }
        transaction.execute("UPDATE trades SET data=?2,deleted=?3 WHERE id=?1", params![id,serde_json::to_string(&next).map_err(|_| "无法保存交易")?,deleted]).map_err(database_error)?;
        save_revision(&transaction, id, action)?;
        let snapshot = snapshot_from(&transaction)?;
        transaction.commit().map_err(database_error)?;
        Ok(snapshot)
    }

    pub fn trade_history(&self, id: &str) -> Result<Vec<TradeRevision>, String> {
        let mut query = self.connection.prepare("SELECT id,trade_id,data,deleted,changed_at,action FROM trade_revisions WHERE trade_id=?1 ORDER BY id DESC").map_err(database_error)?;
        let rows = query.query_map([id], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).map_err(database_error)?.collect::<Result<Vec<_>,_>>().map_err(database_error)?;
        rows.into_iter().map(|(id,trade_id,data,deleted,changed_at,action)| Ok(TradeRevision { id,trade_id,input:serde_json::from_str(&data).map_err(|_| "交易历史损坏")?,deleted,changed_at,action })).collect()
    }

    pub fn value_position(&self, account: &str, market: &str, code: &str, price: &str) -> Result<PositionValue, String> {
        let snapshot = self.snapshot()?;
        let position = snapshot.positions.iter().find(|p| p.account_id == account && p.market == market && p.code == code).ok_or("找不到对应持仓")?;
        value_at_position(position,price)
    }
}

pub(super) fn value_at_position(position:&Position,price:&str)->Result<PositionValue,String>{
 let price=scaled(price,6,false)?;let value=rounded(scaled(&position.quantity,6,true)? as i128*price as i128,10_000_000_000)?;let cost=scaled(&position.cost,2,true)?;
 Ok(PositionValue{market_value:amount_text(value),unrealized:amount_text(value.checked_sub(cost).ok_or(RANGE)?)})
}
