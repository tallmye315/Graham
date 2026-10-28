use graham_ledger::{Ledger,DividendInput};
fn dividend(account:&str)->DividendInput{DividendInput{account_id:account.into(),occurred_on:"2026-10-20".into(),paid_on:None,status:"pending".into(),market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"招商银行".into(),currency:"CNY".into(),gross:"100".into(),tax:"20".into(),net:"80".into(),note:"券商核对".into()}}
#[test]
fn pending_to_paid_credits_cash_once_without_duplicating_dividend_income(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("ledger.sqlite3");let mut ledger=Ledger::open(&path).unwrap();let a=ledger.create_account("分红账户","CNY").unwrap();
 let input=dividend(&a);let first=ledger.record_dividend(&input).unwrap();assert_eq!(first.accounts[0].balance,"0.00");let d=&first.dividends[0];
 let total=&first.dividend_totals[0];assert_eq!(total.net,"80.00");assert_eq!(total.pending,"80.00");assert_eq!(total.paid,"0.00");assert_eq!(total.record_ids,vec![d.id.clone()]);
 let mut paid=input.clone();paid.status="paid".into();paid.paid_on=Some("2026-10-26".into());let result=ledger.revise_dividend(&d.id,d.revision_id,&paid).unwrap();
 assert_eq!(result.accounts[0].balance,"80.00");assert_eq!(result.dividend_totals[0].net,"80.00");assert_eq!(result.dividend_totals[0].pending,"0.00");assert_eq!(result.dividend_totals[0].paid,"80.00");
 assert!(ledger.revise_dividend(&d.id,d.revision_id,&paid).is_err());let d=&result.dividends[0];let result=ledger.revise_dividend(&d.id,d.revision_id,&paid).unwrap();assert_eq!(result.accounts[0].balance,"80.00");assert_eq!(result.dividends.len(),1);
 assert_eq!(ledger.snapshot_on("2026-10-25").unwrap().accounts[0].balance.as_deref(),Some("0.00"));assert_eq!(ledger.snapshot_on("2026-10-26").unwrap().accounts[0].balance.as_deref(),Some("80.00"));
 let flows=ledger.cash_flows(&[a],"2026-10-01","2026-10-31").unwrap();assert_eq!(flows.totals[0].net_capital,"0.00");
 drop(ledger);let ledger=Ledger::open(&path).unwrap();assert_eq!(ledger.snapshot().unwrap().accounts[0].balance,"80.00");assert_eq!(ledger.dividend_history(&d.id).unwrap().len(),3);
}

fn edit_backup(source:&std::path::Path,target:&std::path::Path,password:&str,change:impl FnOnce(&mut serde_json::Value)){
 use age::secrecy::SecretString;let mut identity=age::scrypt::Identity::new(SecretString::from(password.to_owned()));identity.set_max_work_factor(18);
 let mut doc:serde_json::Value=serde_json::from_slice(&age::decrypt(&identity,&std::fs::read(source).unwrap()).unwrap()).unwrap();change(&mut doc);
 let mut recipient=age::scrypt::Recipient::new(SecretString::from(password.to_owned()));recipient.set_work_factor(18);std::fs::write(target,age::encrypt(&recipient,&serde_json::to_vec(&doc).unwrap()).unwrap()).unwrap();
}
#[test]
fn dividend_corrections_validate_dependencies_openings_and_purge_all_history(){
 use graham_ledger::{TradeInput,OpeningInput};
 let dir=tempfile::tempdir().unwrap();let mut ledger=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=ledger.create_account("纠错账户","CNY").unwrap();let mut input=dividend(&a);input.tax="10".into();input.net="90".into();input.status="paid".into();input.paid_on=Some("2026-10-26".into());
 for bad in 0..8{let mut t=input.clone();match bad{0=>t.net="89".into(),1=>{t.tax="101".into();t.net="0".into();},2=>t.paid_on=None,3=>t.paid_on=Some("2026-10-19".into()),4=>t.currency="USD".into(),5=>t.account_id="missing".into(),6=>t.gross="0".into(),_=>t.tax="-1".into()}assert!(ledger.record_dividend(&t).is_err());}
 assert!(ledger.snapshot().unwrap().dividends.is_empty());
 let first=ledger.record_dividend(&input).unwrap();let d=&first.dividends[0];assert_eq!(first.accounts[0].balance,"90.00");assert!(ledger.permanently_delete_dividend(&d.id,d.revision_id).is_err());
 let trade=TradeInput{account_id:a.clone(),occurred_on:"2026-10-26".into(),market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"招商银行".into(),currency:"CNY".into(),side:"buy".into(),quantity:"1".into(),price:"50".into(),fee:"0".into(),tax:"0".into(),note:"".into()};ledger.record_trade(&trade).unwrap();assert_eq!(ledger.snapshot().unwrap().accounts[0].balance,"40.00");
 let mut pending=input.clone();pending.status="pending".into();pending.paid_on=None;assert!(ledger.set_dividend_deleted(&d.id,d.revision_id,true).is_err());assert!(ledger.revise_dividend(&d.id,d.revision_id,&pending).is_err());assert_eq!(ledger.dividend_history(&d.id).unwrap().len(),1);
 let mut wrong=input.clone();wrong.kind="etf".into();assert!(ledger.revise_dividend(&d.id,d.revision_id,&wrong).is_err());assert_eq!(ledger.snapshot().unwrap().accounts[0].balance,"40.00");
 ledger.save_opening(&OpeningInput{account_id:a.clone(),occurred_on:"2026-10-25".into(),cash:"0".into(),positions:vec![],note:"到账前收盘".into()},None).unwrap();assert_eq!(ledger.snapshot().unwrap().accounts[0].balance,"40.00");
 let opening=ledger.snapshot().unwrap();let o=&opening.openings[0];let mut next=o.input.clone();next.occurred_on="2026-10-26".into();next.cash="17".into();ledger.save_opening(&next,Some(o.revision_id)).unwrap();assert_eq!(ledger.snapshot().unwrap().accounts[0].balance,"17.00");
 let deleted=ledger.set_dividend_deleted(&d.id,d.revision_id,true).unwrap();assert_eq!(deleted.accounts[0].balance,"17.00");assert!(deleted.dividend_totals.is_empty());let d=&deleted.dividends[0];
 assert!(ledger.set_dividend_deleted(&d.id,first.dividends[0].revision_id,false).is_err());let restored=ledger.restore_dividend_revision(&d.id,d.revision_id,first.dividends[0].revision_id).unwrap();assert_eq!(restored.dividend_totals[0].net,"90.00");assert_eq!(restored.accounts[0].balance,"17.00");
 let b=ledger.create_account("可清空","CNY").unwrap();let mut pending=dividend(&b);pending.code="601166".into();let created=ledger.record_dividend(&pending).unwrap();let d=created.dividends.iter().find(|d|d.input.account_id==b).unwrap();assert!(ledger.update_account(&b,"可清空","USD").is_err());assert!(ledger.delete_empty_account(&b).is_err());
 let deleted=ledger.set_dividend_deleted(&d.id,d.revision_id,true).unwrap();let d=deleted.dividends.iter().find(|r|r.id==d.id).unwrap();ledger.permanently_delete_dividend(&d.id,d.revision_id).unwrap();assert!(ledger.dividend_history(&d.id).unwrap().is_empty());assert!(ledger.restore_dividend_revision(&d.id,d.revision_id,created.dividends.last().unwrap().revision_id).is_err());ledger.update_account(&b,"空账户","USD").unwrap();ledger.delete_empty_account(&b).unwrap();
 ledger.set_account_archived(&a,true).unwrap();assert!(ledger.record_dividend(&input).is_err());
}
#[test]
fn encrypted_backup_keeps_dividend_states_and_history_and_rejects_missing_sections(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("ledger.sqlite3");let mut ledger=Ledger::open(&path).unwrap();let a=ledger.create_account("备份账户","CNY").unwrap();let password="dividend-backup-password";
 let empty=dir.path().join("empty.grahambak");ledger.export_backup(&empty,password).unwrap();let legacy=dir.path().join("legacy.grahambak");edit_backup(&empty,&legacy,password,|doc|{doc["version"]=serde_json::json!(4);doc["schemaVersion"]=serde_json::json!(5);for k in ["dividends","dividendRevisions","dividendRevisionSequence"]{doc.as_object_mut().unwrap().remove(k);}});
 let pending=ledger.record_dividend(&dividend(&a)).unwrap();let d=&pending.dividends[0];let mut input=d.input.clone();input.status="paid".into();input.paid_on=Some("2026-10-26".into());let paid=ledger.revise_dividend(&d.id,d.revision_id,&input).unwrap();let d=&paid.dividends[0];
 let second=ledger.create_account("美股分红","USD").unwrap();let mut usd=dividend(&second);usd.market="US".into();usd.code="AAPL".into();usd.currency="USD".into();ledger.record_dividend(&usd).unwrap();
 let backup=dir.path().join("all.grahambak");ledger.export_backup(&backup,password).unwrap();let preview=Ledger::preview_backup(&backup,password).unwrap();assert_eq!(preview.record_count,2);assert_eq!(preview.deleted_count,0);
 let invalid=dir.path().join("missing.grahambak");edit_backup(&backup,&invalid,password,|doc|{for k in ["dividends","dividendRevisions","dividendRevisionSequence"]{doc.as_object_mut().unwrap().remove(k);}});assert!(Ledger::preview_backup(&invalid,password).is_err());
 let before=ledger.snapshot().unwrap();assert!(ledger.restore_backup(&invalid,password,"invalid",&dir.path().join("rejected")).is_err());assert_eq!(ledger.snapshot().unwrap().accounts,before.accounts);
 ledger.set_dividend_deleted(&d.id,d.revision_id,true).unwrap();let restored=ledger.restore_backup(&backup,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();assert_eq!(restored.snapshot.accounts,before.accounts);assert_eq!(restored.snapshot.dividends.len(),2);assert_eq!(restored.snapshot.dividend_totals.len(),2);assert_eq!(restored.snapshot.dividend_totals[0].net,"80.00");assert_eq!(ledger.dividend_history(&d.id).unwrap().len(),2);
 assert!(ledger.revise_dividend(&d.id,d.revision_id,&input).is_err());assert!(ledger.restore_dividend_revision(&d.id,restored.snapshot.dividends[0].revision_id,pending.dividends[0].revision_id).is_err());
 let preview=Ledger::preview_backup(&legacy,password).unwrap();let cleared=ledger.restore_backup(&legacy,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();assert!(cleared.snapshot.dividends.is_empty());assert_eq!(cleared.snapshot.accounts[0].balance,"0.00");
 let protection=std::path::Path::new(&cleared.recovery_path);let preview=Ledger::preview_backup(protection,password).unwrap();let recovered=ledger.restore_backup(protection,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();assert_eq!(recovered.snapshot.accounts,before.accounts);assert_eq!(ledger.dividend_history(&d.id).unwrap().len(),2);
 drop(ledger);let reopened=Ledger::open(&path).unwrap();assert_eq!(reopened.snapshot().unwrap().accounts,before.accounts);
}
