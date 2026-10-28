use graham_ledger::{DailyClose, DividendInput, HistoryResponse, IncomeQuery, Ledger, OpeningInput, OpeningPosition, QuoteInput, QuoteKey, QuoteResponse, TradeInput};

fn epoch(value: &str) -> i64 { chrono::DateTime::parse_from_rfc3339(value).unwrap().timestamp() }
fn key() -> QuoteKey { QuoteKey { market: "US".into(), code: "AAPL".into(), kind: "stock".into() } }
fn seed_history(ledger: &mut Ledger, query: &IncomeQuery, now: i64) -> HistoryResponse {
    let job = ledger.begin_income_refresh(query, now).unwrap().unwrap();
    let mut date = chrono::NaiveDate::parse_from_str(&job.start_on, "%Y-%m-%d").unwrap();
    let end = chrono::NaiveDate::parse_from_str(&job.end_on, "%Y-%m-%d").unwrap();
    let mut sessions = vec![];
    while date <= end {
        let mut session = date;
        while chrono::Datelike::weekday(&session).number_from_monday() > 5 { session -= chrono::Duration::days(1); }
        sessions.push(serde_json::json!({"date":date.to_string(),"session":session.to_string(),"reason":null}));
        date += chrono::Duration::days(1);
    }
    let response: HistoryResponse = serde_json::from_value(serde_json::json!({
        "feed":"performance","series":[{"key":key(),"source":"yfinance / Yahoo Finance","basis":"split_checked","timezone":"America/New_York",
        "prices":[{"date":"2026-10-23","close":"108"},{"date":"2026-10-26","close":"110"}],"invalidatesBefore":null,"error":null}],
        "calendars":[{"market":"US","name":"XNYS","version":"4.13.2","sessions":sessions}],"error":null
    })).unwrap();
    ledger.finish_performance_refresh(&job, &response, now + 1).unwrap();
    response
}

#[test]
fn partial_sale_separates_realized_unrealized_dividend_and_today_without_double_charging() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("公开合成样例", "USD").unwrap();
    ledger.save_opening(&OpeningInput { account_id: account.clone(), occurred_on: "2026-10-23".into(), cash: "1000".into(),
        positions: vec![OpeningPosition { market: "US".into(), code: "AAPL".into(), kind: "stock".into(), name: "样例证券".into(), currency: "USD".into(), quantity: "10".into(), cost: "1000".into(), reference_price: None }], note: "".into() }, None).unwrap();
    ledger.record_trade(&TradeInput { account_id: account.clone(), occurred_on: "2026-10-27".into(), market: "US".into(), code: "AAPL".into(), kind: "stock".into(), name: "样例证券".into(), currency: "USD".into(), side: "sell".into(), quantity: "4".into(), price: "115".into(), fee: "2".into(), tax: "0".into(), note: "".into() }).unwrap();
    ledger.record_dividend(&DividendInput { account_id: account.clone(), occurred_on: "2026-10-27".into(), paid_on: None, status: "pending".into(), market: "US".into(), code: "AAPL".into(), kind: "stock".into(), name: "样例证券".into(), currency: "USD".into(), gross: "10".into(), tax: "2".into(), net: "8".into(), note: "".into() }).unwrap();
    let query = IncomeQuery { account_ids: vec![account] };
    let now = epoch("2026-10-27T15:00:00Z");
    seed_history(&mut ledger, &query, now);
    let quote = ledger.begin_quote_refresh(now, false, false).unwrap().jobs.remove(0);
    ledger.finish_quote_job(&quote, &QuoteResponse { feed: "us".into(), quotes: vec![QuoteInput { history: vec![DailyClose { date: "2026-10-26".into(), close: "110".into() }], key: key(), price: "112".into(), currency: "USD".into(), source_time: Some("2026-10-27T11:00:00-04:00".into()), quoted_at: Some("2026-10-27T11:00:00-04:00".into()) }], error: None }, now + 2).unwrap();
    let before = ledger.snapshot().unwrap();
    let state = ledger.income(&query, now + 3).unwrap();
    let market = state.markets.iter().find(|m| m.market == "US").unwrap();
    let row = &market.rows[0];
    assert_eq!(market.trading_date.as_deref(), Some("2026-10-27"));
    assert_eq!(row.quantity, "6");
    assert_eq!(row.amounts.realized, "58.00");
    assert_eq!(row.amounts.unrealized.as_deref(), Some("72.00"));
    assert_eq!(row.amounts.net_dividend, "8.00");
    assert_eq!(row.amounts.pending_dividend, "8.00");
    assert_eq!(row.amounts.fee, "2.00");
    assert_eq!(row.amounts.dividend_tax, "2.00");
    assert_eq!(row.amounts.total.as_deref(), Some("138.00"));
    assert_eq!(market.amounts.today.as_deref(), Some("38.00"));
    assert_eq!(market.amounts.daily_return.as_deref(), Some("3.454545"));
    assert!(!row.closed);
    assert_eq!(ledger.snapshot().unwrap().positions, before.positions);
    assert_eq!(ledger.snapshot().unwrap().accounts, before.accounts);
}

#[test]
fn an_old_intraday_quote_after_close_is_only_an_estimate_not_a_verified_close() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("过期行情合成样例", "USD").unwrap();
    ledger.save_opening(&OpeningInput { account_id: account.clone(), occurred_on: "2026-10-23".into(), cash: "0".into(),
        positions: vec![OpeningPosition { market: "US".into(), code: "AAPL".into(), kind: "stock".into(), name: "样例证券".into(), currency: "USD".into(), quantity: "10".into(), cost: "1000".into(), reference_price: None }], note: "".into() }, None).unwrap();
    let query = IncomeQuery { account_ids: vec![account] };
    let now = epoch("2026-10-27T22:00:00Z");
    seed_history(&mut ledger, &query, now);
    let job = ledger.begin_quote_refresh(now, false, false).unwrap().jobs.remove(0);
    ledger.finish_quote_job(&job, &QuoteResponse { feed: "us".into(), quotes: vec![QuoteInput {
        history: vec![DailyClose { date: "2026-10-26".into(), close: "110".into() }], key: key(), price: "112".into(), currency: "USD".into(),
        source_time: Some("2026-10-27T11:00:00-04:00".into()), quoted_at: Some("2026-10-27T11:00:00-04:00".into()),
    }], error: None }, now + 2).unwrap();
    let state = ledger.income(&query, now + 3).unwrap();
    let market = state.markets.iter().find(|m| m.market == "US").unwrap();
    assert_eq!(market.status, "stale");
    assert_eq!(market.amounts.today.as_deref(), Some("20.00"));
    assert!(market.rows[0].reasons.iter().any(|r| r.contains("过期")));
    assert!(market.rows[0].reasons.iter().any(|r| r.contains("收盘")));
}

#[test]
fn first_day_purchase_uses_actual_cost_and_ignores_idle_cash_for_the_daily_return() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("首日买入合成样例", "USD").unwrap();
    ledger.record_deposit(&account, "2026-10-27", "1000", "").unwrap();
    ledger.record_trade(&TradeInput { account_id: account.clone(), occurred_on: "2026-10-27".into(), market: "US".into(), code: "AAPL".into(), kind: "stock".into(), name: "样例证券".into(), currency: "USD".into(), side: "buy".into(), quantity: "1".into(), price: "100".into(), fee: "1".into(), tax: "0".into(), note: "".into() }).unwrap();
    let query = IncomeQuery { account_ids: vec![account] };
    let now = epoch("2026-10-27T15:00:00Z");
    seed_history(&mut ledger, &query, now);
    let job = ledger.begin_quote_refresh(now, false, false).unwrap().jobs.remove(0);
    ledger.finish_quote_job(&job, &QuoteResponse { feed: "us".into(), quotes: vec![QuoteInput {
        history: vec![DailyClose { date: "2026-10-26".into(), close: "110".into() }], key: key(), price: "112".into(), currency: "USD".into(),
        source_time: Some("2026-10-27T11:00:00-04:00".into()), quoted_at: Some("2026-10-27T11:00:00-04:00".into()),
    }], error: None }, now + 2).unwrap();
    let state = ledger.income(&query, now + 3).unwrap();
    let market = state.markets.iter().find(|m| m.market == "US").unwrap();
    assert_eq!(market.amounts.total.as_deref(), Some("11.00"));
    assert_eq!(market.amounts.today.as_deref(), Some("11.00"));
    assert_eq!(market.amounts.daily_return.as_deref(), Some("10.891089"));
    assert_eq!(market.rows[0].cost, "101.00");
}

#[test]
fn fully_sold_security_retains_realized_income_and_dividend_payment_is_not_new_income() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("清仓合成样例", "USD").unwrap();
    ledger.save_opening(&OpeningInput { account_id: account.clone(), occurred_on: "2026-10-23".into(), cash: "0".into(),
        positions: vec![OpeningPosition { market: "US".into(), code: "AAPL".into(), kind: "stock".into(), name: "样例证券".into(), currency: "USD".into(), quantity: "10".into(), cost: "1000".into(), reference_price: None }], note: "".into() }, None).unwrap();
    ledger.record_trade(&TradeInput { account_id: account.clone(), occurred_on: "2026-10-27".into(), market: "US".into(), code: "AAPL".into(), kind: "stock".into(), name: "样例证券".into(), currency: "USD".into(), side: "sell".into(), quantity: "10".into(), price: "115".into(), fee: "2".into(), tax: "0".into(), note: "".into() }).unwrap();
    ledger.record_dividend(&DividendInput { account_id: account.clone(), occurred_on: "2026-10-26".into(), paid_on: Some("2026-10-27".into()), status: "paid".into(), market: "US".into(), code: "AAPL".into(), kind: "stock".into(), name: "样例证券".into(), currency: "USD".into(), gross: "10".into(), tax: "2".into(), net: "8".into(), note: "".into() }).unwrap();
    let query = IncomeQuery { account_ids: vec![account] };
    let now = epoch("2026-10-28T06:00:00Z");
    seed_history(&mut ledger, &query, now);
    let state = ledger.income(&query, now + 2).unwrap();
    let market = state.markets.iter().find(|m| m.market == "US").unwrap();
    assert_eq!(market.trading_date.as_deref(), Some("2026-10-27"));
    assert_eq!(market.status, "preopen");
    assert!(market.rows[0].closed);
    assert_eq!(market.amounts.realized, "148.00");
    assert_eq!(market.amounts.unrealized.as_deref(), Some("0.00"));
    assert_eq!(market.amounts.paid_dividend, "8.00");
    assert_eq!(market.amounts.today_dividend, "0.00");
    assert_eq!(market.amounts.total.as_deref(), Some("156.00"));
    assert_eq!(market.amounts.today.as_deref(), Some("48.00"));
    assert_eq!(market.amounts.daily_return.as_deref(), Some("4.363636"));
}

#[test]
fn failed_history_refresh_preserves_amounts_but_reports_stale_and_allows_a_backoff_retry() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("历史缓存失败合成样例", "USD").unwrap();
    ledger.save_opening(&OpeningInput { account_id: account.clone(), occurred_on: "2026-10-23".into(), cash: "0".into(),
        positions: vec![OpeningPosition { market: "US".into(), code: "AAPL".into(), kind: "stock".into(), name: "样例证券".into(), currency: "USD".into(), quantity: "10".into(), cost: "1000".into(), reference_price: None }], note: "".into() }, None).unwrap();
    let query = IncomeQuery { account_ids: vec![account] };
    let now = epoch("2026-10-27T22:00:00Z");
    let mut response = seed_history(&mut ledger, &query, now);
    response.series[0].prices.push(DailyClose { date: "2026-10-27".into(), close: "112".into() });
    let good = ledger.begin_income_refresh(&query, now + 901).unwrap().unwrap();
    ledger.finish_performance_refresh(&good, &response, now + 902).unwrap();
    assert_eq!(ledger.income(&query, now + 903).unwrap().markets.iter().find(|m| m.market == "US").unwrap().status, "closed");
    let failed = ledger.begin_income_refresh(&query, now + 1803).unwrap().unwrap();
    ledger.finish_performance_refresh(&failed, &HistoryResponse { feed: "performance".into(), series: vec![], calendars: vec![], error: Some("受控网络失败".into()) }, now + 1804).unwrap();
    let state = ledger.income(&query, now + 1805).unwrap();
    let market = state.markets.iter().find(|m| m.market == "US").unwrap();
    assert_eq!(market.status, "stale");
    assert_eq!(market.amounts.today.as_deref(), Some("20.00"));
    assert!(market.rows[0].reasons.iter().any(|r| r.contains("受控网络失败")));
    assert!(ledger.begin_income_refresh(&query, now + 1865).unwrap().is_some());
}
