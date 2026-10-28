use super::{amount_text, backup, database_error, trades, Ledger, PerformanceJob, Position, QuoteKey, QuoteView, Snapshot};
use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IncomeQuery { pub account_ids: Vec<String> }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomeAmounts {
    pub realized: String, pub unrealized: Option<String>, pub net_dividend: String,
    pub pending_dividend: String, pub paid_dividend: String, pub total: Option<String>,
    pub fee: String, pub tax: String, pub dividend_tax: String,
    pub today: Option<String>, pub today_dividend: String, pub today_fee: String,
    pub today_tax: String, pub today_dividend_tax: String, pub daily_return: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomeRow {
    pub account_id: String, pub account_name: String, pub key: QuoteKey, pub name: String,
    pub quantity: String, pub cost: String, pub closed: bool, pub amounts: IncomeAmounts,
    pub source_time: Option<String>, pub received_at: Option<String>, pub reasons: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketIncome {
    pub market: String, pub currency: String, pub trading_date: Option<String>,
    pub previous_date: Option<String>, pub status: String, pub amounts: IncomeAmounts, pub rows: Vec<IncomeRow>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomeState { pub markets: Vec<MarketIncome>, pub cache_warning: Option<String>, pub method: String }

const RANGE: &str = "收益金额超出支持范围，请缩小账户范围";
fn cents(text: &str) -> Result<i64, String> {
    let (negative, magnitude) = text.strip_prefix('-').map(|v| (true, v)).unwrap_or((false, text));
    let value = trades::scaled(magnitude, 2, true)?;
    Ok(if negative { -value } else { value })
}
fn add(a: i64, b: i64) -> Result<i64, String> { a.checked_add(b).ok_or_else(|| RANGE.into()) }
fn total(values: impl IntoIterator<Item = i64>) -> Result<i64, String> { values.into_iter().try_fold(0, add) }
fn percent(profit: i64, base: i64) -> Option<String> {
    if base <= 0 { return None; }
    let n = ((profit as i128).abs() * 100_000_000 + base as i128 / 2) / base as i128;
    Some(format!("{}{}.{:06}", if profit < 0 && n != 0 { "-" } else { "" }, n / 1_000_000, n % 1_000_000))
}
fn security(position: &Position) -> QuoteKey { QuoteKey { market: position.market.clone(), code: position.code.clone(), kind: position.kind.clone() } }
fn zone(market: &str) -> chrono_tz::Tz {
    match market { "US" => chrono_tz::America::New_York, "HK" => chrono_tz::Asia::Hong_Kong, _ => chrono_tz::Asia::Shanghai }
}
fn window(now: i64) -> Result<(NaiveDate, NaiveDate), String> {
    let date = DateTime::<Utc>::from_timestamp(now, 0).ok_or("系统时间无效")?.with_timezone(&chrono_tz::Asia::Shanghai).date_naive();
    Ok((date - Duration::days(40), date))
}
fn scope(snapshot: &Snapshot, query: &IncomeQuery) -> Result<BTreeSet<String>, String> {
    let ids = if query.account_ids.is_empty() { snapshot.accounts.iter().map(|a| a.id.clone()).collect() } else { query.account_ids.iter().cloned().collect::<BTreeSet<_>>() };
    if !query.account_ids.is_empty() && ids.len() != query.account_ids.len() || ids.iter().any(|id| !snapshot.accounts.iter().any(|a| &a.id == id)) { return Err("账户范围已变化，请刷新".into()); }
    Ok(ids)
}
fn keys(snapshot: &Snapshot, ids: &BTreeSet<String>) -> Vec<QuoteKey> {
    snapshot.positions.iter().filter(|p| ids.contains(&p.account_id)).map(security)
        .chain(snapshot.dividends.iter().filter(|d| !d.deleted && ids.contains(&d.input.account_id)).map(|d| QuoteKey { market: d.input.market.clone(), code: d.input.code.clone(), kind: d.input.kind.clone() }))
        .collect::<BTreeSet<_>>().into_iter().collect()
}

#[derive(Default)]
struct IncomeNumbers {
    realized: i64, unrealized: Option<i64>, dividend: i64, pending: i64, paid: i64,
    fee: i64, tax: i64, dividend_tax: i64, today: Option<i64>, today_dividend: i64,
    today_fee: i64, today_tax: i64, today_dividend_tax: i64, base: Option<i64>,
}
impl IncomeNumbers {
    fn amounts(&self) -> Result<IncomeAmounts, String> {
        Ok(IncomeAmounts {
            realized: amount_text(self.realized), unrealized: self.unrealized.map(amount_text), net_dividend: amount_text(self.dividend),
            pending_dividend: amount_text(self.pending), paid_dividend: amount_text(self.paid),
            total: self.unrealized.map(|u| total([self.realized, u, self.dividend])).transpose()?.map(amount_text),
            fee: amount_text(self.fee), tax: amount_text(self.tax), dividend_tax: amount_text(self.dividend_tax),
            today: self.today.map(amount_text), today_dividend: amount_text(self.today_dividend), today_fee: amount_text(self.today_fee),
            today_tax: amount_text(self.today_tax), today_dividend_tax: amount_text(self.today_dividend_tax),
            daily_return: self.today.zip(self.base).and_then(|(n, b)| percent(n, b)),
        })
    }
    fn merge(&mut self, row: &Self) -> Result<(), String> {
        self.realized = add(self.realized, row.realized)?; self.dividend = add(self.dividend, row.dividend)?;
        self.pending = add(self.pending, row.pending)?; self.paid = add(self.paid, row.paid)?;
        self.fee = add(self.fee, row.fee)?; self.tax = add(self.tax, row.tax)?; self.dividend_tax = add(self.dividend_tax, row.dividend_tax)?;
        self.today_dividend = add(self.today_dividend, row.today_dividend)?; self.today_fee = add(self.today_fee, row.today_fee)?;
        self.today_tax = add(self.today_tax, row.today_tax)?; self.today_dividend_tax = add(self.today_dividend_tax, row.today_dividend_tax)?;
        self.unrealized = self.unrealized.zip(row.unrealized).map(|(a, b)| add(a, b)).transpose()?;
        self.today = self.today.zip(row.today).map(|(a, b)| add(a, b)).transpose()?;
        self.base = self.base.zip(row.base).map(|(a, b)| add(a, b)).transpose()?;
        Ok(())
    }
}

impl Ledger {
    pub fn begin_income_refresh(&mut self, query: &IncomeQuery, now: i64) -> Result<Option<PerformanceJob>, String> {
        let snapshot = self.snapshot()?; let ids = scope(&snapshot, query)?;
        let (start, end) = window(now)?; let cache = self.performance_cache(start, end, now)?;
        let mut ranked = vec![];
        for key in keys(&snapshot, &ids) {
            let local = DateTime::<Utc>::from_timestamp(now, 0).ok_or("系统时间无效")?.with_timezone(&zone(&key.market));
            let today = local.date_naive().to_string();
            let attempt: i64 = self.market.connection.query_row("SELECT COALESCE(MAX(last_attempt),0) FROM valuation_data WHERE key LIKE ?1", [format!("%:{}:{}:{}", key.market, key.code, key.kind)], |r| r.get(0)).map_err(database_error)?;
            let local_date = local.date_naive();
            let lookup = local_date - Duration::days(if local.hour() * 60 + local.minute() < 570 { 1 } else { 0 });
            let trading = cache.session(&key, &lookup.to_string()).and_then(|s| s.0.as_deref());
            let previous = trading.and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).and_then(|d| cache.session(&key, &(d - Duration::days(1)).to_string())).and_then(|s| s.0.as_deref());
            let needs_source = previous.is_none_or(|d| cache.sources(&[key.clone()], d, now).0.iter().any(|s| s.value.is_none() || s.error.is_some()));
            let after_close = trading.is_some_and(|d| d != today || local.hour() * 60 + local.minute() >= if key.market == "CN" { 915 } else { 990 });
            let needs_close = after_close && trading.is_some_and(|d| cache.sources(&[key.clone()], d, now).0.iter().any(|s| s.value.is_none() || s.error.is_some()));
            if cache.session(&key, &today).is_none() || needs_source || needs_close || now - attempt >= 900 { ranked.push((attempt, key)); }
        }
        ranked.sort();
        self.begin_history_keys(start, end, ranked.into_iter().take(10).map(|(_, key)| key).collect(), now)
    }

    pub fn income(&self, query: &IncomeQuery, now: i64) -> Result<IncomeState, String> {
        let frozen = backup::copy_to_memory(&self.connection)?;
        let snapshot = frozen.snapshot()?; let ids = scope(&snapshot, query)?;
        let (start, end) = window(now)?; let cache = self.performance_cache(start, end, now)?;
        let quotes = self.market.views(keys(&snapshot, &ids), now)?;
        let mut markets = vec![];
        for (market, currency) in [("CN", "CNY"), ("US", "USD"), ("HK", "HKD")] {
            let anchor = QuoteKey { market: market.into(), code: String::new(), kind: "stock".into() };
            let local = DateTime::<Utc>::from_timestamp(now, 0).ok_or("系统时间无效")?.with_timezone(&zone(market));
            let minute = local.hour() * 60 + local.minute();
            let lookup = local.date_naive() - Duration::days(if minute < 570 { 1 } else { 0 });
            let trading_date = cache.session(&anchor, &lookup.to_string()).and_then(|s| s.0.clone());
            let previous_date = trading_date.as_ref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).and_then(|d| cache.session(&anchor, &(d - Duration::days(1)).to_string())).and_then(|s| s.0.clone());
            let date = trading_date.clone().unwrap_or_else(|| local.date_naive().to_string());
            let current = frozen.snapshot_on(&date)?;
            let previous = previous_date.as_deref().map(|d| frozen.snapshot_on(d)).transpose()?;
            let mut positions: BTreeMap<(String, QuoteKey), Position> = current.accounts.iter().filter(|a| ids.contains(&a.account_id)).flat_map(|a| a.positions.iter()).filter(|p| p.market == market).map(|p| ((p.account_id.clone(), security(p)), p.clone())).collect();
            for d in snapshot.dividends.iter().filter(|d| !d.deleted && ids.contains(&d.input.account_id) && d.input.market == market && d.input.occurred_on <= date) {
                let t = &d.input; let key = QuoteKey { market: t.market.clone(), code: t.code.clone(), kind: t.kind.clone() };
                positions.entry((t.account_id.clone(), key)).or_insert_with(|| Position { account_id: t.account_id.clone(), market: t.market.clone(), code: t.code.clone(), kind: t.kind.clone(), name: t.name.clone(), currency: t.currency.clone(), quantity: "0".into(), cost: "0.00".into(), average_cost: "0".into(), realized: "0.00".into(), since: None });
            }
            let mut rows = vec![];
            let mut stale = false;
            let mut estimated = false;
            let mut sum = IncomeNumbers { unrealized: Some(0), today: Some(0), base: Some(0), ..Default::default() };
            for ((account_id, key), position) in positions {
                let opening = snapshot.openings.iter().find(|o| !o.deleted && o.input.account_id == account_id);
                let covered = |d: &str| opening.is_some_and(|o| d <= o.input.occurred_on.as_str());
                let in_day = |d: &str| previous_date.as_deref().is_some_and(|base| d > base) && d <= date.as_str();
                let prior = previous.as_ref().and_then(|s| s.accounts.iter().find(|a| a.account_id == account_id)).and_then(|a| a.positions.iter().find(|p| security(p) == key));
                let mut reasons = vec![];
                let quote = quotes.iter().find(|q| q.key == key);
                let daily_source = cache.sources(&[key.clone()], &date, now).0.into_iter().next();
                let price = daily_source.as_ref().and_then(|s| s.value.as_deref()).or_else(|| quote.and_then(|q| q.price.as_deref()));
                let unrealized = if position.quantity == "0" { Some(0) } else { price.map(|p| trades::value_at_position(&position, p).and_then(|v| cents(&v.unrealized))).transpose()? };
                let prior_source = previous_date.as_deref().and_then(|d| cache.sources(&[key.clone()], d, now).0.into_iter().next());
                for (label, source) in [("当日收盘", daily_source.as_ref().filter(|s| s.value.is_some() && position.quantity != "0")), ("前收盘", prior_source.as_ref().filter(|_| prior.is_some_and(|p| p.quantity != "0")))] {
                    if let Some(error) = source.and_then(|s| s.error.as_ref()) {
                        stale = true; reasons.push(format!("{label}来源失败，沿用有效缓存：{error}"));
                    }
                }
                let prior_value = match prior { None => Some(0), Some(p) if p.quantity == "0" => Some(0), Some(p) => prior_source.as_ref().and_then(|s| s.value.as_ref()).map(|price| trades::value_at_position(p, price).and_then(|v| cents(&v.market_value))).transpose()? };
                let mut n = IncomeNumbers { realized: cents(&position.realized)?, unrealized, ..Default::default() };
                let mut previous_dividend = 0;
                for d in snapshot.dividends.iter().filter(|d| !d.deleted && d.input.account_id == account_id && d.input.market == key.market && d.input.code == key.code && d.input.kind == key.kind && d.input.occurred_on <= date && !covered(&d.input.occurred_on)) {
                    let t = &d.input; let net = cents(&t.net)?;
                    n.dividend = add(n.dividend, net)?; n.dividend_tax = add(n.dividend_tax, cents(&t.tax)?)?;
                    if t.status == "paid" && t.paid_on.as_deref().is_some_and(|paid| paid <= date.as_str()) { n.paid = add(n.paid, net)?; } else { n.pending = add(n.pending, net)?; }
                    if in_day(&t.occurred_on) { n.today_dividend = add(n.today_dividend, net)?; n.today_dividend_tax = add(n.today_dividend_tax, cents(&t.tax)?)?; } else { previous_dividend = add(previous_dividend, net)?; }
                }
                let mut purchases = 0;
                for t in snapshot.trades.iter().filter(|t| !t.deleted && t.input.account_id == account_id && t.input.market == key.market && t.input.code == key.code && t.input.kind == key.kind && t.input.occurred_on <= date && !covered(&t.input.occurred_on)) {
                    let t = &t.input; let fee = cents(&t.fee)?; let tax = cents(&t.tax)?;
                    n.fee = add(n.fee, fee)?; n.tax = add(n.tax, tax)?;
                    if in_day(&t.occurred_on) {
                        n.today_fee = add(n.today_fee, fee)?; n.today_tax = add(n.today_tax, tax)?;
                        if t.side == "buy" {
                            let q = trades::scaled(&t.quantity, 6, false)? as i128; let p = trades::scaled(&t.price, 6, false)? as i128;
                            let gross = i64::try_from((q * p + 5_000_000_000) / 10_000_000_000).map_err(|_| RANGE)?;
                            purchases = add(purchases, total([gross, fee, tax])?)?;
                        }
                    }
                }
                let price_matches = position.quantity == "0" || daily_source.as_ref().is_some_and(|s| s.value.is_some()) || quote.is_some_and(|q| quote_date(q, market).as_deref() == Some(date.as_str()));
                if trading_date.is_none() || previous_date.is_none() { reasons.push("交易日历或前一交易日尚未核对，请刷新".into()); }
                if unrealized.is_none() { reasons.push("缺少持仓价格，累计未实现盈亏数据不足".into()); }
                if !price_matches { reasons.push("缓存报价不属于该交易日，今日盈亏数据不足".into()); }
                if prior_value.is_none() { reasons.push("缺少前一交易日收盘价，今日盈亏数据不足".into()); }
                let opening_break = opening.is_some_and(|o| previous_date.as_deref().is_none_or(|base| o.input.occurred_on.as_str() > base));
                if opening_break { reasons.push("收盘期初覆盖本日，无法回推今日盈亏".into()); }
                if previous_date.is_some() && trading_date.is_some() && price_matches && !opening_break {
                    if let (Some(u), Some(base)) = (unrealized, prior_value) {
                        let previous_total = total([prior.map(|p| cents(&p.realized)).transpose()?.unwrap_or(0), base.checked_sub(prior.map(|p| cents(&p.cost)).transpose()?.unwrap_or(0)).ok_or(RANGE)?, previous_dividend])?;
                        n.today = total([n.realized, u, n.dividend])?.checked_sub(previous_total);
                        n.base = Some(add(base, purchases)?);
                    }
                }
                if let Some(q) = quote.filter(|_| position.quantity != "0" && daily_source.as_ref().is_none_or(|s| s.value.is_none())) {
                    estimated = true;
                    if let Some(error) = &q.error { reasons.push(format!("沿用缓存：{error}")); }
                    if q.quoted_at.is_none() { reasons.push("来源未提供完整报价日期，盘中结果为参考估算".into()); }
                    let age = q.quoted_at.as_deref().or(q.received_at.as_deref()).and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map(|t| now - t.timestamp());
                    if quote_date(q, market).as_deref() != Some(date.as_str()) || age.is_none_or(|seconds| seconds > 900) || q.error.is_some() {
                        stale = true; reasons.push("参考报价已过期或来源失败，沿用缓存估算".into());
                    }
                    if date != local.date_naive().to_string() || minute >= if market == "CN" { 900 } else { 960 } {
                        reasons.push("尚无已核对收盘价，显示最近盘中估算".into());
                    }
                }
                sum.merge(&n)?;
                rows.push(IncomeRow { account_name: snapshot.accounts.iter().find(|a| a.id == account_id).map(|a| a.name.clone()).unwrap_or_default(), account_id, key, name: position.name, quantity: position.quantity.clone(), cost: position.cost, closed: position.quantity == "0", amounts: n.amounts()?, source_time: daily_source.as_ref().filter(|s| s.value.is_some()).and_then(|s| s.source_time.clone()).or_else(|| quote.and_then(|q| q.source_time.clone())), received_at: daily_source.as_ref().filter(|s| s.value.is_some()).and_then(|s| s.received_at.clone()).or_else(|| quote.and_then(|q| q.received_at.clone())), reasons });
            }
            let status = if trading_date.is_none() { "missing" } else if stale { "stale" } else if date != local.date_naive().to_string() { if minute < 570 { "preopen" } else { "rest" } } else if minute < if market == "CN" { 900 } else { 960 } { "intraday" } else if estimated { "closing" } else { "closed" };
            if rows.is_empty() { sum.unrealized = None; sum.today = None; sum.base = None; }
            markets.push(MarketIncome { market: market.into(), currency: currency.into(), trading_date, previous_date, status: status.into(), amounts: sum.amounts()?, rows });
        }
        Ok(IncomeState { markets, cache_warning: self.market.warning.clone(), method: "证券原币口径；今日盈亏为前收盘后证券净贡献的变化，当日买卖及税后分红计入。回报率分母为日初证券市值加当日买入总投入（含税费），卖出按日末流出；无参与本金不计算比率。费用明细已含于净收益，不重复扣除。".into() })
    }
}

fn quote_date(quote: &QuoteView, market: &str) -> Option<String> {
    quote.quoted_at.as_deref().or(quote.received_at.as_deref()).and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&zone(market)).date_naive().to_string())
}
