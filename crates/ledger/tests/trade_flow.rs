mod support;
use support::deposit_revision;
use graham_ledger::{Ledger, TradeInput};

fn trade(account: &str, side: &str, quantity: &str, price: &str) -> TradeInput {
    TradeInput { account_id: account.into(), occurred_on: "2026-10-25".into(), market: "CN".into(), code: "600036".into(), kind: "stock".into(), name: "招商银行".into(), currency: "CNY".into(), side: side.into(), quantity: quantity.into(), price: price.into(), fee: "0".into(), tax: "0".into(), note: "".into() }
}

#[test]
fn prd_ac11_ac12_buy_sell_updates_cash_cost_and_realized_results_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let account = ledger.create_account("验收账户", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "5000", "").unwrap();
    ledger.record_trade(&trade(&account, "buy", "100", "10")).unwrap();
    ledger.record_trade(&trade(&account, "buy", "100", "20")).unwrap();
    let bought = ledger.snapshot().unwrap();
    assert_eq!(bought.accounts[0].balance, "2000.00");
    assert_eq!(bought.positions[0].quantity, "200");
    assert_eq!(bought.positions[0].cost, "3000.00");
    assert_eq!(bought.positions[0].average_cost, "15");
    ledger.record_trade(&trade(&account, "sell", "100", "18")).unwrap();
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    let sold = ledger.snapshot().unwrap();
    assert_eq!(sold.accounts[0].balance, "3800.00");
    assert_eq!(sold.positions[0].quantity, "100");
    assert_eq!(sold.positions[0].cost, "1500.00");
    assert_eq!(sold.positions[0].realized, "300.00");
    let value = ledger.value_position(&account, "CN", "600036", "18").unwrap();
    assert_eq!(value.market_value, "1800.00");
    assert_eq!(value.unrealized, "300.00");
    assert_eq!(sold.trades.len(), 3);
    assert!(sold.trades.iter().all(|t| t.source == "手动录入" && t.input.currency == "CNY"));
}

#[test]
fn trading_backup_restores_fees_history_and_invalidates_old_window_handles() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("费用验收", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "5000", "").unwrap();
    for price in ["10", "20"] {
        let mut buy = trade(&account, "buy", "100", price); buy.fee = "4".into(); buy.tax = "1".into();
        ledger.record_trade(&buy).unwrap();
    }
    let mut sell = trade(&account, "sell", "100", "18"); sell.fee = "2".into(); sell.tax = "1".into();
    ledger.record_trade(&sell).unwrap();
    let expected = ledger.snapshot().unwrap();
    assert_eq!(expected.accounts[0].balance, "3787.00");
    assert_eq!(expected.positions[0].cost, "1505.00");
    assert_eq!(expected.positions[0].realized, "292.00");
    let backup = dir.path().join("trades.grahambak");
    let password = "trade-backup-password";
    ledger.export_backup(&backup, password).unwrap();
    let preview = Ledger::preview_backup(&backup, password).unwrap();
    assert_eq!(preview.record_count, 4);
    let old = &expected.trades[2];
    ledger.set_trade_deleted(&old.id, old.revision_id, true).unwrap();
    let restored = ledger.restore_backup(&backup, password, &preview.fingerprint, &dir.path().join("recovery")).unwrap();
    assert_eq!(restored.snapshot.accounts, expected.accounts);
    assert_eq!(restored.snapshot.positions, expected.positions);
    assert_eq!(restored.snapshot.trades.len(), 3);
    assert_eq!(ledger.trade_history(&old.id).unwrap().len(), 1);
    assert!(ledger.set_trade_deleted(&old.id, old.revision_id, true).is_err());
    let current = &restored.snapshot.trades[2];
    assert!(ledger.restore_trade_revision(&current.id, current.revision_id, old.revision_id).is_err());
}

#[test]
fn dependent_corrections_oversells_invalid_inputs_and_stale_edits_are_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let a = ledger.create_account("账户一", "CNY").unwrap();
    let b = ledger.create_account("账户二", "CNY").unwrap();
    ledger.record_deposit(&a, "2026-10-25", "5000", "").unwrap();
    ledger.record_deposit(&b, "2026-10-25", "100", "").unwrap();
    ledger.record_trade(&trade(&a, "buy", "100", "10")).unwrap();
    ledger.record_trade(&trade(&a, "sell", "60", "12")).unwrap();
    let before = ledger.snapshot().unwrap();
    for invalid in [trade(&a,"sell","41","12"), trade(&b,"sell","1","12"), trade(&b,"buy","100","10"), trade(&a,"buy","-1","10"), trade(&a,"buy","1.0000001","10")] { assert!(ledger.record_trade(&invalid).is_err()); }
    let mut wrong = trade(&a,"buy","1","10"); wrong.currency = "USD".into(); assert!(ledger.record_trade(&wrong).is_err());
    wrong.currency = "CNY".into(); wrong.code = "AAPL".into(); assert!(ledger.record_trade(&wrong).is_err());
    let buy = &before.trades[0];
    assert!(ledger.set_trade_deleted(&buy.id, buy.revision_id, true).unwrap_err().contains("超卖"));
    assert!(ledger.revise_trade(&buy.id, buy.revision_id, &trade(&a,"buy","50","10")).is_err());
    let deposit = before.entries.iter().find(|e| e.account_id == a).unwrap();
    assert!(ledger.set_deposit_deleted(&deposit.id, deposit_revision(&ledger, &deposit.id), true).unwrap_err().contains("现金不足"));
    let mut backdated = trade(&a,"sell","1","12"); backdated.occurred_on = "2026-10-24".into();
    assert!(ledger.record_trade(&backdated).is_err());
    assert_eq!(ledger.snapshot().unwrap().trades, before.trades);
    assert_eq!(ledger.snapshot().unwrap().accounts, before.accounts);
    assert_eq!(ledger.trade_history(&buy.id).unwrap().len(), 1);
    let sell = &before.trades[1];
    let changed = ledger.set_trade_deleted(&sell.id, sell.revision_id, true).unwrap();
    assert!(ledger.set_trade_deleted(&sell.id, sell.revision_id, false).is_err());
    let restored = ledger.restore_trade_revision(&sell.id, changed.trades[1].revision_id, sell.revision_id).unwrap();
    assert_eq!(restored.positions, before.positions);
    assert_eq!(restored.accounts, before.accounts);
    assert_eq!(ledger.trade_history(&sell.id).unwrap().len(), 3);
    drop(ledger);
    assert_eq!(Ledger::open(&path).unwrap().snapshot().unwrap().positions, before.positions);
}

#[test]
fn fractional_us_shares_keep_rounding_residuals_and_close_cost_to_zero() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let a = ledger.create_account("美元账户", "USD").unwrap();
    ledger.record_deposit(&a,"2026-10-25","100", "").unwrap();
    let mut t = trade(&a,"buy","0.3","10.01");
    t.market = "US".into(); t.code = "AAPL".into(); t.name = "苹果".into(); t.currency = "USD".into();
    ledger.record_trade(&t).unwrap();
    assert_eq!(ledger.snapshot().unwrap().positions[0].cost, "3.00");
    t.side = "sell".into(); t.quantity = "0.1".into(); t.price = "9.99".into();
    ledger.record_trade(&t).unwrap();
    assert_eq!(ledger.snapshot().unwrap().positions[0].cost, "2.00");
    t.quantity = "0.2".into(); ledger.record_trade(&t).unwrap();
    let closed = ledger.snapshot().unwrap();
    assert_eq!(closed.positions[0].quantity, "0");
    assert_eq!(closed.positions[0].cost, "0.00");
    assert_eq!(closed.positions[0].realized, "0.00");
    assert_eq!(closed.accounts[0].balance, "100.00");
    assert!(ledger.update_account(&a,"美元账户","CNY").is_err());
    assert!(ledger.delete_empty_account(&a).is_err());
}

#[test]
fn legacy_cash_only_backup_replaces_trades_and_protective_copy_retains_them() {
    use age::secrecy::SecretString;
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let a = ledger.create_account("兼容验收", "CNY").unwrap();
    ledger.record_deposit(&a,"2026-10-25","100", "").unwrap();
    let password = "legacy-backup-password";
    let original = dir.path().join("original.grahambak");
    ledger.export_backup(&original, password).unwrap();
    let mut identity = age::scrypt::Identity::new(SecretString::from(password.to_owned())); identity.set_max_work_factor(18);
    let plaintext = age::decrypt(&identity, &std::fs::read(&original).unwrap()).unwrap();
    let mut old: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
    old["version"] = serde_json::json!(1); old["schemaVersion"] = serde_json::json!(2);
    for key in ["trades","tradeRevisions","tradeRevisionSequence"] { old.as_object_mut().unwrap().remove(key); }
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(password.to_owned())); recipient.set_work_factor(18);
    let legacy = dir.path().join("legacy.grahambak");
    std::fs::write(&legacy, age::encrypt(&recipient, &serde_json::to_vec(&old).unwrap()).unwrap()).unwrap();
    ledger.record_trade(&trade(&a,"buy","1","10")).unwrap();
    let before = ledger.snapshot().unwrap();
    let preview = Ledger::preview_backup(&legacy,password).unwrap();
    let result = ledger.restore_backup(&legacy,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();
    assert!(result.snapshot.trades.is_empty());
    assert!(result.snapshot.positions.is_empty());
    assert_eq!(result.snapshot.accounts[0].balance, "100.00");
    let protective = std::path::Path::new(&result.recovery_path);
    let preview = Ledger::preview_backup(protective,password).unwrap();
    let restored = ledger.restore_backup(protective,password,&preview.fingerprint,&dir.path().join("recovery")).unwrap();
    assert_eq!(restored.snapshot.accounts,before.accounts);
    assert_eq!(restored.snapshot.positions,before.positions);
    assert_eq!(restored.snapshot.trades.len(),1);
}

#[test]
fn voided_wrong_security_types_do_not_block_correct_reentry_but_cannot_be_restored_over_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let a = ledger.create_account("类型纠错", "CNY").unwrap();
    ledger.record_deposit(&a,"2026-10-25","100", "").unwrap();
    let mut t = trade(&a,"buy","1","10"); t.code="510300".into(); t.name="沪深300ETF".into();
    ledger.record_trade(&t).unwrap(); ledger.record_trade(&t).unwrap();
    let wrong = ledger.snapshot().unwrap().trades;
    for old in &wrong { ledger.set_trade_deleted(&old.id,old.revision_id,true).unwrap(); }
    t.kind="etf".into();
    let corrected = ledger.record_trade(&t).unwrap();
    assert_eq!(corrected.positions[0].kind,"etf");
    assert_eq!(corrected.positions[0].quantity,"1");
    let deleted = &corrected.trades[0];
    assert!(ledger.set_trade_deleted(&deleted.id,deleted.revision_id,false).is_err());
    assert_eq!(ledger.snapshot().unwrap().trades,corrected.trades);
}

#[test]
fn permanently_deleting_a_voided_trade_removes_history_without_reapplying_cash_or_cost() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let a = ledger.create_account("清空账户", "CNY").unwrap();
    let b = ledger.create_account("保留账户", "CNY").unwrap();
    for account in [&a,&b] { ledger.record_deposit(account,"2026-10-25","100", "").unwrap(); }
    let created = ledger.record_trade(&trade(&a,"buy","1","10")).unwrap();
    let first = &created.trades[0];
    assert!(ledger.permanently_delete_trade(&first.id,first.revision_id).is_err());
    ledger.record_trade(&trade(&b,"buy","1","20")).unwrap();
    let revised = ledger.revise_trade(&first.id,first.revision_id,&trade(&a,"buy","1","11")).unwrap();
    let current = &revised.trades[0];
    let deleted = ledger.set_trade_deleted(&current.id,current.revision_id,true).unwrap();
    let target = &deleted.trades[0];
    assert!(ledger.permanently_delete_trade(&first.id,first.revision_id).is_err());
    assert!(ledger.permanently_delete_trade("missing",target.revision_id).is_err());
    let restored = ledger.set_trade_deleted(&target.id,target.revision_id,false).unwrap();
    assert!(ledger.permanently_delete_trade(&target.id,target.revision_id).is_err());
    let deleted = ledger.set_trade_deleted(&target.id,restored.trades[0].revision_id,true).unwrap();
    let target = &deleted.trades[0];
    let deposit = deleted.entries.iter().find(|e| e.account_id==a).unwrap();
    ledger.set_deposit_deleted(&deposit.id, deposit_revision(&ledger, &deposit.id),true).unwrap();
    let current_deposit = ledger.snapshot().unwrap().entries.into_iter().find(|e| e.id==deposit.id).unwrap();
    ledger.permanently_delete_deposits(&[graham_ledger::RecordSelection{id:current_deposit.id,revision_id:current_deposit.revision_id}]).unwrap();
    assert!(ledger.update_account(&a,"清空账户","USD").is_err());
    let before = ledger.snapshot().unwrap();
    let backup = dir.path().join("before.grahambak");
    ledger.export_backup(&backup,"trade-purge-password").unwrap();
    let backup_bytes = std::fs::read(&backup).unwrap();
    let purged = ledger.permanently_delete_trade(&target.id,target.revision_id).unwrap();
    assert_eq!(purged.positions,before.positions);
    assert_eq!(purged.accounts.iter().map(|a| &a.balance).collect::<Vec<_>>(),before.accounts.iter().map(|a| &a.balance).collect::<Vec<_>>());
    assert_eq!(purged.trades.len(),1);
    assert!(ledger.trade_history(&target.id).unwrap().is_empty());
    assert_eq!(ledger.trade_history(&purged.trades[0].id).unwrap().len(),1);
    assert!(ledger.restore_trade_revision(&target.id,target.revision_id,first.revision_id).is_err());
    assert!(ledger.set_trade_deleted(&target.id,target.revision_id,false).is_err());
    assert!(ledger.permanently_delete_trade(&target.id,target.revision_id).is_err());
    assert_eq!(std::fs::read(&backup).unwrap(),backup_bytes);
    ledger.update_account(&a,"空账户","USD").unwrap();
    ledger.delete_empty_account(&a).unwrap();
    let after = dir.path().join("after.grahambak"); ledger.export_backup(&after,"trade-purge-password").unwrap();
    let preview=Ledger::preview_backup(&after,"trade-purge-password").unwrap(); assert_eq!(preview.record_count,2);
    let mut other=Ledger::open(&dir.path().join("restore.sqlite3")).unwrap();
    let recovered=other.restore_backup(&after,"trade-purge-password",&preview.fingerprint,&dir.path().join("recovery")).unwrap();
    assert_eq!(recovered.snapshot.trades.len(),1); assert!(other.trade_history(&target.id).unwrap().is_empty());
    drop(ledger);
    let reopened=Ledger::open(&path).unwrap(); assert_eq!(reopened.snapshot().unwrap().trades.len(),1); assert!(reopened.trade_history(&target.id).unwrap().is_empty());
}
