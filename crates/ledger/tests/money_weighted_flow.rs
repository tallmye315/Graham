use graham_ledger::{Ledger,OpeningInput,DividendInput,PerformanceQuery};
fn now()->i64{chrono::DateTime::parse_from_rfc3339("2026-10-27T01:00:00Z").unwrap().timestamp()}
fn cash_opening(l:&mut Ledger,name:&str,cash:&str)->String{let a=l.create_account(name,"CNY").unwrap();l.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:"2021-01-01".into(),cash:cash.into(),positions:vec![],note:"".into()},None).unwrap();a}
fn transfer(l:&mut Ledger,a:&str,b:&str,date:&str,amount:&str,fee:&str){l.record_movement(&graham_ledger::MovementInput{from_account_id:a.into(),to_account_id:b.into(),occurred_on:date.into(),kind:"transfer".into(),from_currency:"CNY".into(),to_currency:"CNY".into(),from_amount:amount.into(),to_amount:amount.into(),actual_rate:"1".into(),fee:fee.into(),fee_side:"from".into(),note:"".into()}).unwrap();}
fn income(l:&mut Ledger,a:&str,date:&str,amount:&str){l.record_dividend(&DividendInput{account_id:a.into(),occurred_on:date.into(),paid_on:Some(date.into()),status:"paid".into(),market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"收益核对".into(),currency:"CNY".into(),gross:amount.into(),tax:"0".into(),net:amount.into(),note:"".into()}).unwrap();}
fn query(a:String,end:&str)->PerformanceQuery{PerformanceQuery{account_ids:vec![a],currency:"CNY".into(),range:"custom".into(),start_on:Some("2021-01-01".into()),end_on:Some(end.into())}}
#[test]
fn multiple_roots_no_root_and_near_repeated_root_never_show_a_fabricated_return(){
 for(withdraw,expense,status)in [("230","232","multiple_roots"),("50","120","no_solution"),("200","200","uncertain")]{
  let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=cash_opening(&mut l,"非唯一样例","100");let b=l.create_account("范围外","CNY").unwrap();
  income(&mut l,&a,"2022-01-01",withdraw);transfer(&mut l,&a,&b,"2022-01-01",withdraw,"0");l.record_deposit(&a,"2023-01-01","400", "").unwrap();transfer(&mut l,&a,&b,"2023-01-01","1",expense);
  // Net signed dated cash flows are [-100, 230, -132], [-100,50,-20],
  // and [-100,200,-100]. Their yearly polynomials have two, zero, and a double root.
  let s=l.money_weighted(&query(a,"2023-01-01"),now()).unwrap();assert_eq!(s.status,status,"{:?}",s.reason);assert!(s.reason.is_some());assert!(s.annualized_return.is_none()&&s.period_return.is_none());
 }
}
#[test]
fn transfer_fees_follow_scope_and_corrections_recompute_negative_and_zero_returns(){
 let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=cash_opening(&mut l,"甲","100");let b=cash_opening(&mut l,"乙","100");transfer(&mut l,&a,&b,"2022-01-01","50","10");
 let mut q=query(a.clone(),"2022-01-01");assert_eq!(l.money_weighted(&q,now()).unwrap().annualized_return.as_deref(),Some("-10.000000"));q.account_ids=vec![b];assert_eq!(l.money_weighted(&q,now()).unwrap().annualized_return.as_deref(),Some("0.000000"));q.account_ids=vec![];assert_eq!(l.money_weighted(&q,now()).unwrap().annualized_return.as_deref(),Some("-5.000000"));
 let snapshot=l.snapshot().unwrap();let m=&snapshot.movements[0];let mut fixed=m.input.clone();fixed.fee="0".into();l.revise_movement(&m.id,m.revision_id,&fixed).unwrap();assert_eq!(l.money_weighted(&q,now()).unwrap().annualized_return.as_deref(),Some("0.000000"));
 q.start_on=Some("2022-01-01".into());assert_eq!(l.money_weighted(&q,now()).unwrap().elapsed_days,1);q.start_on=Some("2021-01-01".into());q.end_on=q.start_on.clone();let same=l.money_weighted(&q,now()).unwrap();assert_eq!(same.status,"insufficient_period");assert!(same.annualized_return.is_none());
 let empty=cash_opening(&mut l,"零本金","0");let s=l.money_weighted(&query(empty,"2022-01-01"),now()).unwrap();assert_eq!(s.status,"no_initial_capital");assert!(s.annualized_return.is_none());
}

fn history_response(key:graham_ledger::QuoteKey,start:&str,end:&str,prices:&[(&str,&str)])->graham_ledger::HistoryResponse{
 use chrono::{NaiveDate,Duration};let first=NaiveDate::parse_from_str(start,"%Y-%m-%d").unwrap();let last=NaiveDate::parse_from_str(end,"%Y-%m-%d").unwrap();let fx=key.kind=="fx";
 serde_json::from_value(serde_json::json!({"feed":"performance","series":[{"key":key,"source":if fx{"yfinance / Yahoo Finance"}else{"AKShare / 新浪"},"basis":if fx{"daily_close"}else{"unadjusted"},"timezone":if fx{"Europe/London"}else{"Asia/Shanghai"},"prices":prices.iter().map(|(date,close)|serde_json::json!({"date":date,"close":close})).collect::<Vec<_>>(),"invalidatesBefore":null,"error":null}],"calendars":[{"market":if fx{"FX"}else{"CN"},"name":if fx{"FX_WEEKDAYS"}else{"XSHG"},"version":if fx{"1"}else{"4.13.2"},"sessions":(0..=(last-first).num_days()).map(|d|{let date=(first+Duration::days(d)).to_string();let supplied=prices.iter().any(|p|p.0==date);serde_json::json!({"date":date,"session":if supplied{Some(date.as_str())}else{None},"reason":if supplied{None}else{Some("本样例不提供中间日线")}})}).collect::<Vec<_>>()}],"error":null})).unwrap()
}
#[test]
fn endpoint_valuation_is_sufficient_when_unrelated_mid_period_security_prices_are_missing(){
 use graham_ledger::{OpeningPosition,QuoteKey};let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=l.create_account("端点核对","CNY").unwrap();l.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:"2021-01-04".into(),cash:"0".into(),positions:vec![OpeningPosition{market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"核对证券".into(),currency:"CNY".into(),quantity:"10".into(),cost:"100".into(),reference_price:None}],note:"".into()},None).unwrap();l.record_deposit(&a,"2022-01-04","100", "").unwrap();let mut q=query(a,"2023-01-04");q.start_on=Some("2021-01-04".into());
 let missing=l.money_weighted(&q,now()).unwrap();assert_eq!(missing.status,"missing");assert!(missing.annualized_return.is_none());let job=l.begin_money_weighted_refresh(&q,now()).unwrap().unwrap();l.finish_performance_refresh(&job,&history_response(QuoteKey{market:"CN".into(),code:"600036".into(),kind:"stock".into()},"2021-01-04","2023-01-04",&[("2021-01-04","10"),("2023-01-04","13.1")]),now()+1).unwrap();
 let state=l.money_weighted(&q,now()+2).unwrap();assert_eq!(state.annualized_return.as_deref(),Some("10.000000"));assert_eq!(state.refresh.pending,0);assert!(l.performance(&q,now()+2).unwrap().total_return.is_none());
}
#[test]
fn fx_on_each_capital_date_is_required_and_amounts_remain_in_their_original_currency(){
 use graham_ledger::QuoteKey;let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=l.create_account("美元核对","USD").unwrap();l.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:"2021-01-04".into(),cash:"100".into(),positions:vec![],note:"".into()},None).unwrap();l.record_deposit(&a,"2022-01-04","100", "").unwrap();let mut q=query(a,"2023-01-04");q.start_on=Some("2021-01-04".into());let before=l.snapshot().unwrap();
 let key=QuoteKey{market:"USD".into(),code:"CNY".into(),kind:"fx".into()};let job=l.begin_money_weighted_refresh(&q,now()).unwrap().unwrap();l.finish_performance_refresh(&job,&history_response(key.clone(),"2021-01-04","2023-01-04",&[("2021-01-04","7"),("2023-01-04","9")]),now()+1).unwrap();assert!(l.money_weighted(&q,now()+2).unwrap().annualized_return.is_none());
 let job=l.begin_money_weighted_refresh(&q,now()+65).unwrap().unwrap();l.finish_performance_refresh(&job,&history_response(key,"2021-01-04","2023-01-04",&[("2021-01-04","7"),("2022-01-04","8"),("2023-01-04","9")]),now()+66).unwrap();let s=l.money_weighted(&q,now()+67).unwrap();assert_eq!(s.annualized_return.as_deref(),Some("13.091076"));assert_eq!(s.period_return.as_deref(),Some("27.895914"));assert_eq!(s.flows.iter().map(|f|f.amount.as_deref()).collect::<Vec<_>>(),vec![Some("-700.00"),Some("-800.00"),Some("1800.00")]);assert_eq!(l.snapshot().unwrap().accounts,before.accounts);
}

#[test]
fn default_money_period_ends_at_completed_close_while_custom_date_remains_strict(){
 use graham_ledger::QuoteKey;let checked=chrono::DateTime::parse_from_rfc3339("2026-10-28T09:00:00Z").unwrap().timestamp();let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=l.create_account("默认外汇截止","USD").unwrap();l.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:"2025-10-27".into(),cash:"100".into(),positions:vec![],note:"".into()},None).unwrap();
 let mut q=PerformanceQuery{account_ids:vec![a],currency:"CNY".into(),range:"custom".into(),start_on:Some("2025-10-27".into()),end_on:Some("2026-10-27".into())};let job=l.begin_money_weighted_refresh(&q,checked).unwrap().unwrap();l.finish_performance_refresh(&job,&history_response(QuoteKey{market:"USD".into(),code:"CNY".into(),kind:"fx".into()},"2025-10-27","2026-10-27",&[("2025-10-27","7"),("2026-10-27","7.7")]),checked+1).unwrap();l.record_deposit(&q.account_ids[0],"2026-10-28","100", "截止日之后").unwrap();q.range="all".into();q.start_on=None;q.end_on=None;
 let default=l.money_weighted(&q,checked+2).unwrap();assert_eq!(default.end_on,"2026-10-27");assert_eq!(default.annualized_return.as_deref(),Some("10.000000"));assert!(default.warnings.iter().any(|w|w.contains("已完成收盘")));
 q.range="custom".into();q.start_on=Some("2025-10-27".into());q.end_on=Some("2026-10-28".into());let strict=l.money_weighted(&q,checked+2).unwrap();assert_eq!(strict.end_on,"2026-10-28");assert!(strict.annualized_return.is_none());
 q.range="all".into();q.currency="USD".into();q.start_on=None;q.end_on=None;let cash=l.money_weighted(&q,checked+2).unwrap();assert_eq!(cash.end_on,"2026-10-28");assert_eq!(cash.annualized_return.as_deref(),Some("0.000000"));
}
#[test]
fn new_year_default_does_not_compute_previous_year_return_before_any_completed_session(){
 let checked=chrono::DateTime::parse_from_rfc3339("2023-01-01T01:00:00Z").unwrap().timestamp();let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=l.create_account("年初边界","CNY").unwrap();l.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:"2022-12-30".into(),cash:"0".into(),positions:vec![graham_ledger::OpeningPosition{market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"边界核对".into(),currency:"CNY".into(),quantity:"10".into(),cost:"100".into(),reference_price:None}],note:"".into()},None).unwrap();
 let mut q=PerformanceQuery{account_ids:vec![a],currency:"CNY".into(),range:"custom".into(),start_on:Some("2022-12-30".into()),end_on:Some("2022-12-31".into())};let job=l.begin_money_weighted_refresh(&q,checked).unwrap().unwrap();let response=serde_json::from_value(serde_json::json!({"feed":"performance","series":[{"key":{"market":"CN","code":"600036","kind":"stock"},"source":"AKShare / 新浪","basis":"unadjusted","timezone":"Asia/Shanghai","prices":[{"date":"2022-12-30","close":"10"}],"invalidatesBefore":null,"error":null}],"calendars":[{"market":"CN","name":"XSHG","version":"4.13.2","sessions":[{"date":"2022-12-30","session":"2022-12-30","reason":null},{"date":"2022-12-31","session":"2022-12-30","reason":null}]}],"error":null})).unwrap();l.finish_performance_refresh(&job,&response,checked+1).unwrap();q.range="ytd".into();q.start_on=None;q.end_on=None;
 let s=l.money_weighted(&q,checked+2).unwrap();assert_eq!(s.elapsed_days,0);assert!(s.annualized_return.is_none());assert!(s.warnings.iter().any(|w|w.contains("尚无已完成收盘日期")));
}
#[test]
fn alternating_withdrawal_and_contribution_can_still_have_one_certifiable_return(){
 let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=cash_opening(&mut l,"范围内","100");let b=l.create_account("范围外","CNY").unwrap();
 transfer(&mut l,&a,&b,"2022-01-01","50","0");l.record_deposit(&a,"2023-01-01","20", "").unwrap();income(&mut l,&a,"2024-01-01","24.60");
 let s=l.money_weighted(&query(a,"2024-01-01"),now()).unwrap();assert_eq!(s.status,"ok");assert_eq!(s.annualized_return.as_deref(),Some("10.000000"));assert_eq!(s.period_return.as_deref(),Some("33.100000"));assert_eq!(s.flows.iter().map(|f|f.amount.as_deref()).collect::<Vec<_>>(),vec![Some("-100.00"),Some("50.00"),Some("-20.00"),Some("94.60")]);
}
#[test]
fn independently_worked_annualized_return_accounts_for_capital_amount_and_actual_dates(){
 let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=l.create_account("资金回报样例","CNY").unwrap();
 l.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:"2021-01-01".into(),cash:"1000".into(),positions:vec![],note:"".into()},None).unwrap();l.record_deposit(&a,"2022-01-01","1000", "追加本金").unwrap();
 l.record_dividend(&DividendInput{account_id:a.clone(),occurred_on:"2023-01-01".into(),paid_on:Some("2023-01-01".into()),status:"paid".into(),market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"现金分红核对".into(),currency:"CNY".into(),gross:"310".into(),tax:"0".into(),net:"310".into(),note:"".into()}).unwrap();
 let before=l.snapshot().unwrap();let q=PerformanceQuery{account_ids:vec![a],currency:"CNY".into(),range:"custom".into(),start_on:Some("2021-01-01".into()),end_on:Some("2023-01-01".into())};let state=l.money_weighted(&q,now()).unwrap();
 assert_eq!(state.status,"ok");assert_eq!(state.elapsed_days,730);assert_eq!(state.annualized_return.as_deref(),Some("10.000000"));assert_eq!(state.period_return.as_deref(),Some("21.000000"));assert_eq!(state.flows.iter().map(|f|f.amount.as_deref()).collect::<Vec<_>>(),vec![Some("-1000.00"),Some("-1000.00"),Some("2310.00")]);assert_eq!(state.flows[1].date,"2022-01-01");assert_eq!(l.snapshot().unwrap().accounts,before.accounts);
}
