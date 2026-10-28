mod support;
use support::deposit_revision;
use graham_ledger::{Ledger,MovementInput};
fn movement(from:&str,to:&str)->MovementInput{MovementInput{from_account_id:from.into(),to_account_id:to.into(),occurred_on:"2026-10-26".into(),kind:"transfer".into(),from_currency:"CNY".into(),to_currency:"CNY".into(),from_amount:"10000".into(),to_amount:"10000".into(),actual_rate:"1".into(),fee:"0".into(),fee_side:"from".into(),note:"核对转账".into()}}
#[test]
fn linked_transfers_and_fx_update_both_cash_balances_and_classify_capital_by_scope(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("ledger.sqlite3");let mut ledger=Ledger::open(&path).unwrap();
 let a=ledger.create_account("华龙验收","CNY").unwrap();let b=ledger.create_account("兴业验收","CNY").unwrap();let c=ledger.create_account("美元验收","USD").unwrap();
 ledger.record_deposit(&a,"2026-10-26","100000", "").unwrap();let transfer=movement(&a,&b);ledger.record_movement(&transfer).unwrap();
 let first=ledger.snapshot().unwrap();assert_eq!(first.accounts[0].balance,"90000.00");assert_eq!(first.accounts[1].balance,"10000.00");
 let all=ledger.cash_flows(&[a.clone(),b.clone()],"2026-01-01","2026-12-31").unwrap();assert_eq!(all.totals[0].net_capital,"100000.00");assert_eq!(all.totals[0].fees,"0.00");assert_eq!(all.internal_count,1);
 let receiver=ledger.cash_flows(&[b.clone()],"2026-01-01","2026-12-31").unwrap();assert_eq!(receiver.totals[0].inflow,"10000.00");assert_eq!(receiver.totals[0].net_capital,"10000.00");
 let mut with_fee=transfer.clone();with_fee.fee="10".into();ledger.record_movement(&with_fee).unwrap();
 let second=ledger.snapshot().unwrap();assert_eq!(second.accounts[0].balance,"79990.00");assert_eq!(second.accounts[1].balance,"20000.00");
 let mut fx=movement(&a,&c);fx.kind="exchange".into();fx.to_currency="USD".into();fx.from_amount="7200".into();fx.to_amount="1000".into();fx.actual_rate="0.13888889".into();fx.fee="7.20".into();
 ledger.record_movement(&fx).unwrap();drop(ledger);let ledger=Ledger::open(&path).unwrap();let result=ledger.snapshot().unwrap();
 assert_eq!(result.accounts[0].balance,"72782.80");assert_eq!(result.accounts[2].balance,"1000.00");assert_eq!(result.movements.len(),3);assert_eq!(result.movements[2].input.actual_rate,"0.13888889");
 let all=ledger.cash_flows(&[a,b,c],"2026-01-01","2026-12-31").unwrap();assert!(all.unavailable_reason.is_none());assert_eq!(all.internal_count,3);
 let cny=all.totals.iter().find(|t|t.currency=="CNY").unwrap();assert_eq!(cny.net_capital,"100000.00");assert_eq!(cny.fees,"17.20");
 let usd=all.totals.iter().find(|t|t.currency=="USD").unwrap();assert_eq!(usd.net_capital,"0.00");
}

fn rewrite_backup(source:&std::path::Path,target:&std::path::Path,password:&str,change:impl FnOnce(&mut serde_json::Value)){
 use age::secrecy::SecretString;let mut identity=age::scrypt::Identity::new(SecretString::from(password.to_owned()));identity.set_max_work_factor(18);
 let mut doc:serde_json::Value=serde_json::from_slice(&age::decrypt(&identity,&std::fs::read(source).unwrap()).unwrap()).unwrap();change(&mut doc);
 let mut recipient=age::scrypt::Recipient::new(SecretString::from(password.to_owned()));recipient.set_work_factor(18);std::fs::write(target,age::encrypt(&recipient,&serde_json::to_vec(&doc).unwrap()).unwrap()).unwrap();
}
#[test]
fn movement_backups_keep_pair_history_and_cross_type_order_and_restore_legacy_ledgers(){
 use graham_ledger::TradeInput;
 let dir=tempfile::tempdir().unwrap();let mut ledger=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=ledger.create_account("人民币转出","CNY").unwrap();let b=ledger.create_account("人民币转入","CNY").unwrap();let c=ledger.create_account("美元账户","USD").unwrap();
 ledger.record_deposit(&a,"2026-10-26","1000", "").unwrap();let password="movement-backup-password";
 let legacy_source=dir.path().join("legacy-source.grahambak");ledger.export_backup(&legacy_source,password).unwrap();let legacy=dir.path().join("legacy.grahambak");
 rewrite_backup(&legacy_source,&legacy,password,|doc|{doc["version"]=serde_json::json!(3);doc["schemaVersion"]=serde_json::json!(4);for key in ["movements","movementRevisions","movementRevisionSequence"]{doc.as_object_mut().unwrap().remove(key);}});
 let mut t=movement(&a,&b);t.from_amount="200".into();t.to_amount="200".into();ledger.record_movement(&t).unwrap();
 let early=dir.path().join("early.grahambak");ledger.export_backup(&early,password).unwrap();assert_eq!(Ledger::preview_backup(&early,password).unwrap().record_count,2);
 let invalid=dir.path().join("missing-pair.grahambak");rewrite_backup(&early,&invalid,password,|doc|{for key in ["movements","movementRevisions","movementRevisionSequence"]{doc.as_object_mut().unwrap().remove(key);}});
 assert!(Ledger::preview_backup(&invalid,password).unwrap_err().contains("不完整"));let unchanged=ledger.snapshot().unwrap();assert!(ledger.restore_backup(&invalid,password,"irrelevant",&dir.path().join("rejected")).is_err());assert_eq!(ledger.snapshot().unwrap().accounts,unchanged.accounts);
 let mut trade=TradeInput{account_id:b.clone(),occurred_on:"2026-10-26".into(),market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"招商银行".into(),currency:"CNY".into(),side:"buy".into(),quantity:"1".into(),price:"100".into(),fee:"0".into(),tax:"0".into(),note:"".into()};ledger.record_trade(&trade).unwrap();trade.side="sell".into();trade.price="110".into();ledger.record_trade(&trade).unwrap();
 let mut back=movement(&b,&a);back.from_amount="205".into();back.to_amount="205".into();ledger.record_movement(&back).unwrap();assert_eq!(ledger.snapshot().unwrap().accounts[1].balance,"5.00");
 let mut fx=movement(&a,&c);fx.kind="exchange".into();fx.from_amount="720".into();fx.to_amount="100".into();fx.to_currency="USD".into();fx.actual_rate="0.13888889".into();fx.fee="1".into();fx.fee_side="to".into();ledger.record_movement(&fx).unwrap();
 let before=ledger.snapshot().unwrap();assert_eq!(before.accounts[0].balance,"285.00");assert_eq!(before.accounts[2].balance,"99.00");let old=&before.movements[2];
 let backup=dir.path().join("all.grahambak");ledger.export_backup(&backup,password).unwrap();let preview=Ledger::preview_backup(&backup,password).unwrap();
 ledger.set_movement_deleted(&old.id,old.revision_id,true).unwrap();let restored=ledger.restore_backup(&backup,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();assert_eq!(restored.snapshot.accounts,before.accounts);assert_eq!(restored.snapshot.positions,before.positions);assert_eq!(restored.snapshot.movements.iter().map(|m|m.event_order).collect::<Vec<_>>(),before.movements.iter().map(|m|m.event_order).collect::<Vec<_>>());assert_eq!(ledger.movement_history(&old.id).unwrap().len(),1);
 assert!(ledger.set_movement_deleted(&old.id,old.revision_id,true).is_err());assert!(ledger.restore_movement_revision(&old.id,restored.snapshot.movements[2].revision_id,old.revision_id).is_err());
 let preview=Ledger::preview_backup(&legacy,password).unwrap();let cleared=ledger.restore_backup(&legacy,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();assert!(cleared.snapshot.movements.is_empty());assert!(cleared.snapshot.trades.is_empty());assert_eq!(cleared.snapshot.accounts[0].balance,"1000.00");
 let protection=std::path::Path::new(&cleared.recovery_path);let preview=Ledger::preview_backup(protection,password).unwrap();let restored=ledger.restore_backup(protection,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();assert_eq!(restored.snapshot.accounts,before.accounts);assert_eq!(restored.snapshot.positions,before.positions);
}
#[test]
fn invalid_and_dependent_cash_changes_are_atomic_and_openings_cover_each_leg_independently(){
 use graham_ledger::{TradeInput,OpeningInput};
 let dir=tempfile::tempdir().unwrap();let mut ledger=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=ledger.create_account("来源","CNY").unwrap();let b=ledger.create_account("目标","CNY").unwrap();ledger.record_deposit(&a,"2026-10-26","1000", "").unwrap();
 let mut t=movement(&a,&b);t.from_amount="600".into();t.to_amount="600".into();let first=ledger.record_movement(&t).unwrap();let m=&first.movements[0];
 for bad in 0..6{let mut input=t.clone();match bad{0=>input.to_account_id=a.clone(),1=>input.to_account_id="missing".into(),2=>input.to_currency="USD".into(),3=>input.actual_rate="2".into(),4=>{input.from_amount="5000".into();input.to_amount="5000".into();},_=>{input.fee_side="to".into();input.fee="601".into();}}assert!(ledger.record_movement(&input).is_err());}
 let trade=TradeInput{account_id:b.clone(),occurred_on:"2026-10-26".into(),market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"招商银行".into(),currency:"CNY".into(),side:"buy".into(),quantity:"1".into(),price:"500".into(),fee:"0".into(),tax:"0".into(),note:"".into()};ledger.record_trade(&trade).unwrap();let before=ledger.snapshot().unwrap();
 assert!(ledger.set_movement_deleted(&m.id,m.revision_id,true).is_err());let mut revised=t.clone();revised.from_amount="400".into();revised.to_amount="400".into();assert!(ledger.revise_movement(&m.id,m.revision_id,&revised).is_err());assert!(ledger.permanently_delete_movement(&m.id,m.revision_id).is_err());let deposit=&before.entries[0];assert!(ledger.set_deposit_deleted(&deposit.id, deposit_revision(&ledger, &deposit.id),true).is_err());assert_eq!(ledger.snapshot().unwrap().accounts,before.accounts);assert_eq!(ledger.movement_history(&m.id).unwrap().len(),1);
 ledger.save_opening(&OpeningInput{account_id:b.clone(),occurred_on:"2026-10-26".into(),cash:"0".into(),positions:vec![],note:"收盘已覆盖转入及买卖".into()},None).unwrap();let covered=ledger.snapshot().unwrap();assert_eq!(covered.accounts[0].balance,"400.00");assert_eq!(covered.accounts[1].balance,"0.00");
 assert!(ledger.cash_flows(&[a.clone(),b.clone()],"2026-10-26","2026-10-26").unwrap().unavailable_reason.is_some());let deleted=ledger.set_movement_deleted(&m.id,m.revision_id,true).unwrap();assert_eq!(deleted.accounts[0].balance,"1000.00");assert_eq!(deleted.accounts[1].balance,"0.00");assert!(ledger.set_movement_deleted(&m.id,m.revision_id,false).is_err());ledger.restore_movement_revision(&m.id,deleted.movements[0].revision_id,m.revision_id).unwrap();
 let c=ledger.create_account("待清空","CNY").unwrap();let mut small=movement(&a,&c);small.from_amount="100".into();small.to_amount="100".into();let created=ledger.record_movement(&small).unwrap();let pair=created.movements.iter().find(|m|m.input.to_account_id==c).unwrap();let deleted=ledger.set_movement_deleted(&pair.id,pair.revision_id,true).unwrap();assert!(ledger.update_account(&c,"待清空","USD").is_err());let pair=deleted.movements.iter().find(|m|m.id==pair.id).unwrap();ledger.permanently_delete_movement(&pair.id,pair.revision_id).unwrap();assert!(ledger.movement_history(&pair.id).unwrap().is_empty());assert!(ledger.restore_movement_revision(&pair.id,pair.revision_id,pair.revision_id).is_err());ledger.update_account(&c,"空账户","USD").unwrap();ledger.delete_empty_account(&c).unwrap();
}
