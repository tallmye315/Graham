use graham_ledger::{Ledger, OpeningInput, OpeningPosition, TradeInput};
fn opening(account: &str) -> OpeningInput { OpeningInput { account_id:account.into(), occurred_on:"2026-01-31".into(), cash:"1000".into(), positions:vec![OpeningPosition {market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"招商银行".into(),currency:"CNY".into(),quantity:"100".into(),cost:"1500".into(),reference_price:Some("18".into())}],note:"券商收盘核对".into() } }
fn sell(account: &str) -> TradeInput { TradeInput { account_id:account.into(),occurred_on:"2026-02-01".into(),market:"CN".into(),code:"600036".into(),kind:"stock".into(),name:"招商银行".into(),currency:"CNY".into(),side:"sell".into(),quantity:"40".into(),price:"20".into(),fee:"2".into(),tax:"1".into(),note:"".into() } }
#[test]
fn verified_closing_opening_covers_old_records_and_drives_later_costs_without_inventing_earlier_history() {
 let dir=tempfile::tempdir().unwrap(); let path=dir.path().join("ledger.sqlite3"); let mut ledger=Ledger::open(&path).unwrap();
 let a=ledger.create_account("期初账户","CNY").unwrap();
 ledger.record_deposit(&a,"2026-01-31","5000", "旧记录保留").unwrap();
 let saved=ledger.save_opening(&opening(&a),None).unwrap(); assert_eq!(saved.accounts[0].balance,"1000.00"); assert_eq!(saved.positions[0].cost,"1500.00"); assert_eq!(saved.entries.len(),1);
 ledger.record_trade(&sell(&a)).unwrap(); drop(ledger);
 let ledger=Ledger::open(&path).unwrap(); let current=ledger.snapshot().unwrap();
 assert_eq!(current.accounts[0].balance,"1797.00"); assert_eq!(current.positions[0].quantity,"60"); assert_eq!(current.positions[0].cost,"900.00"); assert_eq!(current.positions[0].realized,"197.00");
 let earlier=ledger.snapshot_on("2026-01-30").unwrap(); assert!(earlier.accounts[0].balance.is_none()); assert!(earlier.accounts[0].unavailable_reason.as_ref().unwrap().contains("数据不足"));
 let closing=ledger.snapshot_on("2026-01-31").unwrap(); assert_eq!(closing.accounts[0].balance.as_deref(),Some("1000.00")); assert_eq!(closing.accounts[0].positions[0].quantity,"100");
 let later=ledger.snapshot_on("2026-02-01").unwrap(); assert_eq!(later.accounts[0].balance.as_deref(),Some("1797.00"));
}

#[test]
fn opening_backup_restores_history_invalidates_old_handles_and_accepts_legacy_backup() {
 use age::secrecy::SecretString;
 let dir=tempfile::tempdir().unwrap();let mut ledger=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=ledger.create_account("期初备份","CNY").unwrap();
 ledger.record_deposit(&a,"2026-01-01","5000", "").unwrap();
 let password="opening-backup-password";let legacy=dir.path().join("legacy.grahambak");ledger.export_backup(&legacy,password).unwrap();
 let mut identity=age::scrypt::Identity::new(SecretString::from(password.to_owned()));identity.set_max_work_factor(18);
 let mut doc:serde_json::Value=serde_json::from_slice(&age::decrypt(&identity,&std::fs::read(&legacy).unwrap()).unwrap()).unwrap();
 doc["version"]=serde_json::json!(2);doc["schemaVersion"]=serde_json::json!(3);for key in ["openings","openingRevisions","openingRevisionSequence"]{doc.as_object_mut().unwrap().remove(key);}
 let mut recipient=age::scrypt::Recipient::new(SecretString::from(password.to_owned()));recipient.set_work_factor(18);
 std::fs::write(&legacy,age::encrypt(&recipient,&serde_json::to_vec(&doc).unwrap()).unwrap()).unwrap();
 ledger.save_opening(&opening(&a),None).unwrap();ledger.record_trade(&sell(&a)).unwrap();let before=ledger.snapshot().unwrap();let old=&before.openings[0];
 let backup=dir.path().join("opening.grahambak");ledger.export_backup(&backup,password).unwrap();let preview=Ledger::preview_backup(&backup,password).unwrap();assert_eq!(preview.record_count,3);
 let mut changed=opening(&a);changed.cash="2000".into();ledger.save_opening(&changed,Some(old.revision_id)).unwrap();
 let restored=ledger.restore_backup(&backup,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();assert_eq!(restored.snapshot.accounts,before.accounts);assert_eq!(restored.snapshot.positions,before.positions);assert_eq!(restored.snapshot.openings[0].input,old.input);
 assert!(ledger.save_opening(&changed,Some(old.revision_id)).is_err());assert!(ledger.restore_opening_revision(&old.id,restored.snapshot.openings[0].revision_id,old.revision_id).is_err());assert_eq!(ledger.opening_history(&old.id).unwrap().len(),1);
 let preview=Ledger::preview_backup(&legacy,password).unwrap();let cleared=ledger.restore_backup(&legacy,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();assert!(cleared.snapshot.openings.is_empty());assert!(cleared.snapshot.trades.is_empty());assert_eq!(cleared.snapshot.accounts[0].balance,"5000.00");
 let protected=std::path::Path::new(&cleared.recovery_path);let preview=Ledger::preview_backup(protected,password).unwrap();let protected=ledger.restore_backup(protected,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();assert_eq!(protected.snapshot.positions,before.positions);assert_eq!(protected.snapshot.accounts,before.accounts);
}

#[test]
fn opening_corrections_recheck_dependencies_and_voided_history_protects_accounts_until_purged() {
 let dir=tempfile::tempdir().unwrap();let mut ledger=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=ledger.create_account("纠错账户","CNY").unwrap();
 ledger.record_deposit(&a,"2026-01-01","5000", "").unwrap();ledger.save_opening(&opening(&a),None).unwrap();ledger.record_trade(&sell(&a)).unwrap();let before=ledger.snapshot().unwrap();let o=&before.openings[0];
 assert!(ledger.save_opening(&opening(&a),None).is_err());let mut invalid=opening(&a);invalid.positions[0].quantity="30".into();assert!(ledger.save_opening(&invalid,Some(o.revision_id)).is_err());
 invalid=opening(&a);invalid.positions.push(invalid.positions[0].clone());assert!(ledger.save_opening(&invalid,Some(o.revision_id)).is_err());
 invalid=opening(&a);invalid.positions[0].cost="-1".into();assert!(ledger.save_opening(&invalid,Some(o.revision_id)).is_err());
 assert!(ledger.set_opening_deleted(&o.id,o.revision_id,true).is_err());assert!(ledger.permanently_delete_opening(&o.id,o.revision_id).is_err());assert_eq!(ledger.snapshot().unwrap().openings,before.openings);assert_eq!(ledger.opening_history(&o.id).unwrap().len(),1);
 // Covered historical deposits must not overflow an unrelated current opening cash balance.
 ledger.record_deposit(&a,"2026-01-02","92233720368547758.07", "已由期初覆盖").unwrap();assert_eq!(ledger.snapshot().unwrap().accounts[0].balance,"1797.00");
 let sell=&before.trades[0];ledger.set_trade_deleted(&sell.id,sell.revision_id,true).unwrap();
 // Removing the opening would expose an overflowing earlier timeline, so it remains atomic.
 assert!(ledger.set_opening_deleted(&o.id,o.revision_id,true).is_err());
 let b=ledger.create_account("纯期初账户","HKD").unwrap();let empty=OpeningInput{account_id:b.clone(),occurred_on:"2026-01-31".into(),cash:"0".into(),positions:vec![],note:"".into()};let created=ledger.save_opening(&empty,None).unwrap();let e=created.openings.iter().find(|o|o.input.account_id==b).unwrap();
 let deleted=ledger.set_opening_deleted(&e.id,e.revision_id,true).unwrap();assert!(ledger.update_account(&b,"纯期初账户","USD").is_err());let e=deleted.openings.iter().find(|o|o.input.account_id==b).unwrap();
 ledger.permanently_delete_opening(&e.id,e.revision_id).unwrap();assert!(ledger.opening_history(&e.id).unwrap().is_empty());ledger.update_account(&b,"空账户","USD").unwrap();ledger.delete_empty_account(&b).unwrap();
}

#[test]
fn incomplete_authenticated_v3_backup_cannot_silently_drop_the_opening_boundary() {
 use age::secrecy::SecretString;
 let dir=tempfile::tempdir().unwrap();let mut ledger=Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();let a=ledger.create_account("完整性验收","CNY").unwrap();
 ledger.record_deposit(&a,"2026-01-01","5000", "").unwrap();ledger.save_opening(&opening(&a),None).unwrap();
 let backup=dir.path().join("valid.grahambak");let password="opening-integrity-password";ledger.export_backup(&backup,password).unwrap();
 let mut identity=age::scrypt::Identity::new(SecretString::from(password.to_owned()));identity.set_max_work_factor(18);
 let mut doc:serde_json::Value=serde_json::from_slice(&age::decrypt(&identity,&std::fs::read(&backup).unwrap()).unwrap()).unwrap();
 for key in ["openings","openingRevisions","openingRevisionSequence"]{doc.as_object_mut().unwrap().remove(key);}
 let mut recipient=age::scrypt::Recipient::new(SecretString::from(password.to_owned()));recipient.set_work_factor(18);
 let bad=dir.path().join("incomplete.grahambak");std::fs::write(&bad,age::encrypt(&recipient,&serde_json::to_vec(&doc).unwrap()).unwrap()).unwrap();
 let before=ledger.snapshot().unwrap();assert!(Ledger::preview_backup(&bad,password).unwrap_err().contains("不完整"));
 assert!(ledger.restore_backup(&bad,password,"irrelevant",&dir.path().join("recovery")).unwrap_err().contains("不完整"));
 assert_eq!(ledger.snapshot().unwrap().accounts,before.accounts);assert_eq!(ledger.snapshot().unwrap().openings,before.openings);assert!(!dir.path().join("recovery").exists());
}
