use graham_ledger::{Ledger,OpeningInput,OpeningPosition,PerformanceQuery,HistoryResponse};
fn now()->i64{chrono::DateTime::parse_from_rfc3339("2026-10-26T13:40:00Z").unwrap().timestamp()}
fn historical(prices:&[(&str,&str)],sessions:&[(&str,&str)])->HistoryResponse{
 serde_json::from_value(serde_json::json!({"feed":"performance","series":[{"key":{"market":"CN","code":"600036","kind":"stock"},"source":"AKShare / 新浪","basis":"unadjusted","timezone":"Asia/Shanghai","prices":prices.iter().map(|(date,close)|serde_json::json!({"date":date,"close":close})).collect::<Vec<_>>(),"invalidatesBefore":null,"error":null}],"calendars":[{"market":"CN","name":"XSHG","version":"4.13.2","sessions":sessions.iter().map(|(date,session)|serde_json::json!({"date":date,"session":session,"reason":null})).collect::<Vec<_>>()}],"error":null})).unwrap()
}
fn opening(l:&mut Ledger,date:&str)->String{
 let a=l.create_account("表现核对","CNY").unwrap();l.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:date.into(),cash:"0".into(),positions:vec![OpeningPosition{market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"招商银行".into(),currency:"CNY".into(),quantity:"10".into(),cost:"100".into(),reference_price:None}],note:"".into()},None).unwrap();a
}
fn query(a:String,start:&str,end:&str)->PerformanceQuery{PerformanceQuery{account_ids:vec![a],currency:"CNY".into(),range:"custom".into(),start_on:Some(start.into()),end_on:Some(end.into())}}

#[test]
fn unadjusted_ex_dividend_price_and_net_receivable_then_payment_do_not_double_count(){
 use graham_ledger::DividendInput;
 let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=opening(&mut l,"2026-10-19");
 l.record_dividend(&DividendInput{account_id:a.clone(),occurred_on:"2026-10-20".into(),paid_on:Some("2026-10-21".into()),status:"paid".into(),market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"招商银行".into(),currency:"CNY".into(),gross:"10".into(),tax:"0".into(),net:"10".into(),note:"".into()}).unwrap();
 let q=query(a,"2026-10-19","2026-10-21");let job=l.begin_performance_refresh(&q,now()).unwrap().unwrap();l.finish_performance_refresh(&job,&historical(&[("2026-10-19","10"),("2026-10-20","9"),("2026-10-21","9")],&[("2026-10-19","2026-10-19"),("2026-10-20","2026-10-20"),("2026-10-21","2026-10-21")]),now()+1).unwrap();
 let s=l.performance(&q,now()+2).unwrap();assert_eq!(s.total_return.as_deref(),Some("0.000000"));for p in s.points{assert_eq!(p.value.as_deref(),Some("100.00"));assert_eq!(p.inflow.as_deref(),Some("0.00"));}
}

#[test]
fn verified_weekend_carries_close_but_missing_session_breaks_cumulative_chain(){
 let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=opening(&mut l,"2026-10-23");let q=query(a,"2026-10-23","2026-10-26");
 let job=l.begin_performance_refresh(&q,now()).unwrap().unwrap();l.finish_performance_refresh(&job,&historical(&[("2026-10-23","10")],&[("2026-10-23","2026-10-23"),("2026-10-24","2026-10-23"),("2026-10-25","2026-10-23"),("2026-10-26","2026-10-26")]),now()+1).unwrap();
 let state=l.performance(&q,now()+2).unwrap();assert_eq!(state.points[2].cumulative_return.as_deref(),Some("0.000000"));assert!(state.points[2].reasons.iter().any(|r|r.contains("休市参考 2026-10-23")));assert_eq!(state.points[3].status,"missing");assert!(state.points[3].value.is_none());assert!(state.total_return.is_none());
 let mut strict=q.clone();strict.start_on=Some("2026-10-24".into());strict.end_on=Some("2026-10-25".into());let weekend=l.performance(&strict,now()+2).unwrap();assert_eq!(weekend.total_return.as_deref(),Some("0.000000"));assert_eq!(weekend.baseline_on.as_deref(),Some("2026-10-23"));
 assert!(l.valuation(&graham_ledger::ValuationQuery{account_ids:q.account_ids,currency:"CNY".into(),date:Some("2026-10-25".into())},now()+2,false).unwrap().total.is_none());
}
#[test]
fn independently_worked_daily_twr_neutralizes_deposit_then_links_market_return(){
 let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=l.create_account("表现样例","CNY").unwrap();l.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:"2026-10-19".into(),cash:"0".into(),positions:vec![OpeningPosition{market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"招商银行".into(),currency:"CNY".into(),quantity:"10".into(),cost:"100".into(),reference_price:None}],note:"".into()},None).unwrap();l.record_deposit(&a,"2026-10-20","100", "追加本金").unwrap();let before=l.snapshot().unwrap();let q=PerformanceQuery{account_ids:vec![a],currency:"CNY".into(),range:"custom".into(),start_on:Some("2026-10-19".into()),end_on:Some("2026-10-21".into())};assert!(l.performance(&q,now()).unwrap().total_return.is_none());let job=l.begin_performance_refresh(&q,now()).unwrap().unwrap();
 let response:HistoryResponse=serde_json::from_value(serde_json::json!({"feed":"performance","series":[{"key":{"market":"CN","code":"600036","kind":"stock"},"source":"AKShare / 新浪","basis":"unadjusted","timezone":"Asia/Shanghai","prices":[{"date":"2026-10-19","close":"10"},{"date":"2026-10-20","close":"10"},{"date":"2026-10-21","close":"11"}],"invalidatesBefore":null,"error":null}],"calendars":[{"market":"CN","name":"XSHG","version":"4.13.2","sessions":[{"date":"2026-10-19","session":"2026-10-19","reason":null},{"date":"2026-10-20","session":"2026-10-20","reason":null},{"date":"2026-10-21","session":"2026-10-21","reason":null}]}],"error":null})).unwrap();l.finish_performance_refresh(&job,&response,now()+1).unwrap();let state=l.performance(&q,now()+2).unwrap();assert!(state.complete);assert_eq!(state.total_return.as_deref(),Some("5.000000"));assert_eq!(state.points[0].status,"baseline");assert_eq!(state.points[1].inflow.as_deref(),Some("100.00"));assert_eq!(state.points[1].daily_return.as_deref(),Some("0.000000"));assert_eq!(state.points[2].value.as_deref(),Some("210.00"));assert_eq!(state.points[2].daily_return.as_deref(),Some("5.000000"));assert_eq!(l.snapshot().unwrap().accounts,before.accounts);assert_eq!(l.snapshot().unwrap().positions,before.positions);
}

#[test]
fn one_unavailable_history_batch_cannot_starve_other_holdings(){
 let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=l.create_account("多批历史","CNY").unwrap();
 l.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:"2026-10-19".into(),cash:"0".into(),positions:(1..=11).map(|n|OpeningPosition{market:"CN".into(),code:format!("600{n:03}"),kind:"stock".into(),name:format!("证券{n}"),currency:"CNY".into(),quantity:"1".into(),cost:"10".into(),reference_price:None}).collect(),note:"".into()},None).unwrap();let q=query(a,"2026-10-19","2026-10-20");
 let first=l.begin_performance_refresh(&q,now()).unwrap().unwrap();assert_eq!(first.keys.len(),10);l.finish_performance_refresh(&first,&HistoryResponse{feed:"performance".into(),series:vec![],calendars:vec![],error:Some("首批缺项".into())},now()+1).unwrap();
 let next=l.begin_performance_refresh(&q,now()+65).unwrap().unwrap();assert!(next.keys.iter().all(|key|!first.keys.contains(key)),"A failed batch must yield to unattempted holdings");
}

#[test]
fn failed_history_recheck_keeps_valid_value_and_discloses_source_time_and_error(){
 let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=opening(&mut l,"2026-10-19");let q=query(a,"2026-10-19","2026-10-20");
 let first=l.begin_performance_refresh(&q,now()).unwrap().unwrap();l.finish_performance_refresh(&first,&historical(&[("2026-10-19","10"),("2026-10-20","11")],&[("2026-10-19","2026-10-19"),("2026-10-20","2026-10-20")]),now()+1).unwrap();let next=l.begin_performance_refresh(&q,now()+65).unwrap().unwrap();l.finish_performance_refresh(&next,&HistoryResponse{feed:"performance".into(),series:vec![],calendars:vec![],error:Some("核对服务暂时失败".into())},now()+66).unwrap();
 let state=l.performance(&q,now()+67).unwrap();assert_eq!(state.total_return.as_deref(),Some("10.000000"));let reasons=state.points[1].reasons.join(" ");assert!(reasons.contains("600036")&&reasons.contains("2026-10-20")&&reasons.contains("核对服务暂时失败"),"{reasons}");
}

#[test]
fn transfer_scope_excludes_principal_but_keeps_fee_and_record_corrections_recompute_history(){
 use graham_ledger::MovementInput;
 let dir=tempfile::tempdir().unwrap();let mut l=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
 let a=l.create_account("转出","CNY").unwrap();let b=l.create_account("转入","CNY").unwrap();
 for(id,cash)in [(&a,"1000"),(&b,"100")]{l.save_opening(&OpeningInput{account_id:id.clone(),occurred_on:"2026-10-19".into(),cash:cash.into(),positions:vec![],note:"".into()},None).unwrap();}
 l.record_movement(&MovementInput{from_account_id:a.clone(),to_account_id:b.clone(),occurred_on:"2026-10-20".into(),kind:"transfer".into(),from_currency:"CNY".into(),to_currency:"CNY".into(),from_amount:"100".into(),to_amount:"100".into(),actual_rate:"1".into(),fee:"10".into(),fee_side:"from".into(),note:"".into()}).unwrap();
 let mut q=PerformanceQuery{account_ids:vec![a.clone(),b.clone()],currency:"CNY".into(),range:"custom".into(),start_on:Some("2026-10-19".into()),end_on:Some("2026-10-20".into())};
 let all=l.performance(&q,now()).unwrap();assert_eq!(all.total_return.as_deref(),Some("-0.909091"));assert_eq!(all.points[1].inflow.as_deref(),Some("0.00"));assert_eq!(all.points[1].outflow.as_deref(),Some("0.00"));
 q.account_ids=vec![a];assert_eq!(l.performance(&q,now()).unwrap().total_return.as_deref(),Some("-1.000000"));
 q.account_ids=vec![b];assert_eq!(l.performance(&q,now()).unwrap().total_return.as_deref(),Some("0.000000"));
 let original=l.snapshot().unwrap();let m=&original.movements[0];let mut corrected=m.input.clone();corrected.fee="0".into();l.revise_movement(&m.id,m.revision_id,&corrected).unwrap();q.account_ids=vec![];
 assert_eq!(l.performance(&q,now()).unwrap().total_return.as_deref(),Some("0.000000"));
}
