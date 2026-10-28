mod support;
use support::deposit_revision;
use graham_ledger::Ledger;

#[test]
fn correcting_a_deposit_recalculates_cash_and_retains_original_values_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let account = ledger.create_account("华龙证券", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-24", "1200.00", "误填").unwrap();
    let id = ledger.snapshot().unwrap().entries[0].id.clone();
    ledger.revise_deposit(&id, deposit_revision(&ledger, &id), "2026-10-25", "125.30", "核对后修正").unwrap();
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.accounts[0].balance, "125.30");
    assert_eq!(snapshot.entries[0].note, "核对后修正");
    let history = ledger.entry_history(&id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].amount, "125.30");
    assert_eq!(history[1].amount, "1200.00");
    assert_eq!(history[1].note, "误填");
}

#[test]
fn rejected_changes_and_restore_overflow_leave_money_and_history_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&directory.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("港币账户", "HKD").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "92233720368547758.07", "原值").unwrap();
    let original = ledger.snapshot().unwrap().entries.remove(0);
    ledger.revise_deposit(&original.id, deposit_revision(&ledger, &original.id), "2026-10-25", "1", "修正").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "1", "另一笔").unwrap();
    let other = ledger.snapshot().unwrap().entries.into_iter().find(|e| e.id != original.id).unwrap();
    for (date,amount,note) in [("2026-02-30","2",""), ("2026-10-25","1.001",""), ("2026-10-25","0",""), ("2026-10-25","2",&"字".repeat(501))] {
        assert!(ledger.revise_deposit(&original.id, deposit_revision(&ledger, &original.id),date,amount,note).is_err());
    }
    assert!(ledger.restore_deposit_revision(&original.id, deposit_revision(&ledger, &original.id), original.revision_id).is_err());
    assert!(ledger.restore_deposit_revision(&other.id, deposit_revision(&ledger, &other.id), original.revision_id).is_err());
    assert!(ledger.revise_deposit("不存在", deposit_revision(&ledger, "不存在"), "2026-10-25", "1", "").is_err());
    assert_eq!(ledger.snapshot().unwrap().accounts[0].balance, "2.00");
    assert_eq!(ledger.entry_history(&original.id).unwrap().len(), 2);
    ledger.set_deposit_deleted(&original.id, deposit_revision(&ledger, &original.id), true).unwrap();
    assert!(ledger.revise_deposit(&original.id, deposit_revision(&ledger, &original.id), "2026-10-25", "5", "").is_err());
    assert!(ledger.restore_deposit_revision(&original.id, deposit_revision(&ledger, &original.id), original.revision_id).is_err());
    assert!(ledger.snapshot().unwrap().entries.iter().find(|e| e.id==original.id).unwrap().deleted);
    assert_eq!(ledger.snapshot().unwrap().accounts[0].balance, "1.00");
    assert_eq!(ledger.entry_history(&original.id).unwrap().len(), 3);
}

#[test]
fn upgrading_a_v1_ledger_preserves_records_and_creates_a_reopenable_original_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    std::fs::write(&path, include_bytes!("fixtures/v1-ledger.sqlite3")).unwrap();
    let mut ledger = Ledger::open(&path).unwrap();
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.accounts.len(), 1);
    assert_eq!(snapshot.accounts[0].balance, "1234.56");
    assert_eq!(snapshot.entries[0].source, "手动录入");
    let entry = &snapshot.entries[0];
    assert_eq!(ledger.entry_history(&entry.id).unwrap()[0].amount, "1234.56");
    ledger.revise_deposit(&entry.id, deposit_revision(&ledger, &entry.id), "2026-10-25", "12.34", "修正").unwrap();
    drop(ledger);
    assert_eq!(Ledger::open(&path).unwrap().snapshot().unwrap().accounts[0].balance, "12.34");
    let backups: Vec<_> = std::fs::read_dir(directory.path()).unwrap().map(|e| e.unwrap().path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("ledger-before-v2-")).collect();
    assert_eq!(backups.len(), 1);
    let backup = Ledger::open(&backups[0]).unwrap().snapshot().unwrap();
    assert_eq!(backup.accounts[0].balance, "1234.56");
    assert_eq!(backup.entries[0].note, "验收数据，不代表实际资金");
}

#[test]
fn accounts_can_be_corrected_or_archived_without_erasing_historical_money() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let empty = ledger.create_account("填错的空账户", "CNY").unwrap();
    ledger.update_account(&empty, "美元账户", "USD").unwrap();
    assert_eq!(ledger.snapshot().unwrap().accounts[0].currency, "USD");
    ledger.delete_empty_account(&empty).unwrap();
    assert!(ledger.snapshot().unwrap().accounts.is_empty());
    let account = ledger.create_account("名称填错", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "10.00", "").unwrap();
    assert!(ledger.update_account(&account, "名称", "USD").is_err());
    ledger.update_account(&account, "兴业证券", "CNY").unwrap();
    ledger.set_account_archived(&account, true).unwrap();
    assert!(ledger.delete_empty_account(&account).is_err());
    assert!(ledger.record_deposit(&account, "2026-10-25", "1", "").is_err());
    drop(ledger);
    let mut ledger = Ledger::open(&path).unwrap();
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.accounts[0].name, "兴业证券");
    assert_eq!(snapshot.accounts[0].balance, "10.00");
    assert!(snapshot.accounts[0].archived);
    ledger.set_account_archived(&account, false).unwrap();
    let entry = &ledger.snapshot().unwrap().entries[0];
    ledger.set_deposit_deleted(&entry.id, deposit_revision(&ledger, &entry.id), true).unwrap();
    assert!(ledger.delete_empty_account(&account).is_err());
    assert!(ledger.snapshot().unwrap().accounts[0].has_history);
}

#[test]
fn deleted_deposit_is_excluded_from_cash_and_can_be_recovered_from_history_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let account = ledger.create_account("美元账户", "USD").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "100.01", "误填").unwrap();
    let entry = ledger.snapshot().unwrap().entries.remove(0);
    ledger.set_deposit_deleted(&entry.id, deposit_revision(&ledger, &entry.id), true).unwrap();
    assert_eq!(ledger.snapshot().unwrap().accounts[0].balance, "0.00");
    drop(ledger);
    let mut ledger = Ledger::open(&path).unwrap();
    assert!(ledger.snapshot().unwrap().entries[0].deleted);
    ledger.restore_deposit_revision(&entry.id, deposit_revision(&ledger, &entry.id), entry.revision_id).unwrap();
    let snapshot = ledger.snapshot().unwrap();
    assert!(!snapshot.entries[0].deleted);
    assert_eq!(snapshot.accounts[0].balance, "100.01");
    let history = ledger.entry_history(&entry.id).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].action, "恢复版本");
    assert!(history[1].deleted);
}

#[test]
fn stale_deposit_commands_cannot_overwrite_a_newer_window_revision() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut first = Ledger::open(&path).unwrap();
    let account = first.create_account("版本核对", "CNY").unwrap();
    first.record_deposit(&account, "2026-10-25", "100", "原始").unwrap();
    let selected = first.snapshot().unwrap().entries.remove(0);
    let mut other = Ledger::open(&path).unwrap();
    other.revise_deposit(&selected.id, selected.revision_id, "2026-10-25", "200", "另一窗口更新").unwrap();
    let before = other.snapshot().unwrap();
    let history = other.entry_history(&selected.id).unwrap();
    assert!(first.set_deposit_deleted(&selected.id, selected.revision_id, true).is_err());
    assert!(first.restore_deposit_revision(&selected.id, selected.revision_id, selected.revision_id).is_err());
    assert!(first.revise_deposit(&selected.id, selected.revision_id, "2026-10-25", "300", "过期表单").is_err());
    let after = first.snapshot().unwrap();
    assert_eq!(after.accounts, before.accounts);
    assert_eq!(after.entries, before.entries);
    assert_eq!(serde_json::to_value(first.entry_history(&selected.id).unwrap()).unwrap(), serde_json::to_value(history).unwrap());
}

#[test]
fn deposit_correction_returns_its_committed_version_for_safe_undo() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut first = Ledger::open(&path).unwrap();
    let account = first.create_account("修订结果", "CNY").unwrap();
    first.record_deposit(&account, "2026-10-25", "100", "原值").unwrap();
    let original = first.snapshot().unwrap().entries.remove(0);
    let saved = first.revise_deposit(&original.id, original.revision_id, "2026-10-25", "200", "本次修改").unwrap();
    assert_eq!(saved.accounts[0].balance, "200.00");
    let selected = &saved.entries[0];
    assert_eq!(selected.amount, "200.00");
    let mut other = Ledger::open(&path).unwrap();
    other.revise_deposit(&selected.id, selected.revision_id, "2026-10-25", "300", "后续修改").unwrap();
    assert!(first.restore_deposit_revision(&selected.id, selected.revision_id, original.revision_id).is_err());
    assert_eq!(first.snapshot().unwrap().accounts[0].balance, "300.00");
    assert_eq!(first.entry_history(&selected.id).unwrap().len(), 3);
}
