mod support;
use support::deposit_revision;
use graham_ledger::{Entry, Ledger, RecordSelection};

fn selection(entries: &[Entry]) -> Vec<RecordSelection> {
    entries.iter().map(|e| RecordSelection { id: e.id.clone(), revision_id: e.revision_id }).collect()
}

#[test]
fn permanently_deleted_record_and_its_history_cannot_be_recovered_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let account = ledger.create_account("待清理账户", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "12.30", "错误记录").unwrap();
    let original = ledger.snapshot().unwrap().entries.remove(0);
    ledger.set_deposit_deleted(&original.id, deposit_revision(&ledger, &original.id), true).unwrap();
    ledger.permanently_delete_deposits(&selection(&ledger.snapshot().unwrap().entries)).unwrap();
    drop(ledger);

    let mut ledger = Ledger::open(&path).unwrap();
    let snapshot = ledger.snapshot().unwrap();
    assert!(snapshot.entries.is_empty());
    assert_eq!(snapshot.accounts[0].balance, "0.00");
    assert!(!snapshot.accounts[0].has_history);
    assert!(ledger.entry_history(&original.id).unwrap().is_empty());
    assert!(ledger.restore_deposit_revision(&original.id, deposit_revision(&ledger, &original.id), original.revision_id).is_err());
    assert!(ledger.set_deposit_deleted(&original.id, deposit_revision(&ledger, &original.id), false).is_err());
    ledger.update_account(&account, "重新整理", "USD").unwrap();
    ledger.delete_empty_account(&account).unwrap();
    assert!(ledger.snapshot().unwrap().accounts.is_empty());
}

#[test]
fn batch_discard_and_recovery_can_each_be_undone_as_one_operation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let account = ledger.create_account("资金账户", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "10", "第一笔").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "20", "第二笔").unwrap();
    let undo = ledger.set_deposits_deleted(&selection(&ledger.snapshot().unwrap().entries), true).unwrap();
    assert_eq!(ledger.snapshot().unwrap().accounts[0].balance, "0.00");
    assert!(ledger.snapshot().unwrap().entries.iter().all(|e| e.deleted));
    ledger.undo_deposit_batch(&undo).unwrap();
    assert_eq!(ledger.snapshot().unwrap().accounts[0].balance, "30.00");
    ledger.set_deposits_deleted(&selection(&ledger.snapshot().unwrap().entries), true).unwrap();
    let undo_restore = ledger.set_deposits_deleted(&selection(&ledger.snapshot().unwrap().entries), false).unwrap();
    assert_eq!(ledger.snapshot().unwrap().accounts[0].balance, "30.00");
    ledger.undo_deposit_batch(&undo_restore).unwrap();
    drop(ledger);
    let mut ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.snapshot().unwrap().accounts[0].balance, "0.00");
    let entry = ledger.snapshot().unwrap().entries.remove(0);
    let original = ledger.entry_history(&entry.id).unwrap().pop().unwrap();
    ledger.restore_deposit_revision(&entry.id, deposit_revision(&ledger, &entry.id), original.id).unwrap();
    assert_eq!(ledger.snapshot().unwrap().accounts[0].balance, "20.00");
}

#[test]
fn invalid_or_stale_batches_leave_all_records_money_and_history_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let account = ledger.create_account("账户", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "10", "第一笔").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "20", "第二笔").unwrap();
    let active = selection(&ledger.snapshot().unwrap().entries);
    assert!(ledger.permanently_delete_deposits(&active).is_err());
    let undo = ledger.set_deposits_deleted(&active, true).unwrap();
    let deleted = selection(&ledger.snapshot().unwrap().entries);
    let mut other_window = Ledger::open(&path).unwrap();
    other_window.set_deposit_deleted(&deleted[1].id, deposit_revision(&other_window, &deleted[1].id), false).unwrap();
    other_window.set_deposit_deleted(&deleted[1].id, deposit_revision(&other_window, &deleted[1].id), true).unwrap();
    let before = ledger.snapshot().unwrap();
    let histories: Vec<_> = before.entries.iter().map(|e| ledger.entry_history(&e.id).unwrap().len()).collect();
    let missing = RecordSelection { id: "不存在".into(), revision_id: 1 };
    for invalid in [vec![], vec![deleted[0].clone(), deleted[0].clone()], vec![deleted[0].clone(), missing], deleted] {
        assert!(ledger.permanently_delete_deposits(&invalid).is_err());
        assert!(ledger.set_deposits_deleted(&invalid, false).is_err());
        let after = ledger.snapshot().unwrap();
        assert_eq!(after.entries, before.entries);
        assert_eq!(after.accounts, before.accounts);
    }
    assert!(ledger.undo_deposit_batch(&undo).is_err());
    assert_eq!(ledger.snapshot().unwrap().entries, before.entries);
    assert_eq!(ledger.snapshot().unwrap().accounts, before.accounts);
    for (entry, count) in before.entries.iter().zip(histories) {
        assert_eq!(ledger.entry_history(&entry.id).unwrap().len(), count);
    }
    ledger.permanently_delete_deposits(&selection(&before.entries)).unwrap();
    assert!(ledger.undo_deposit_batch(&undo).is_err());
    assert!(ledger.snapshot().unwrap().entries.is_empty());
}

#[test]
fn restore_overflow_rolls_back_the_whole_batch_and_its_new_history() {
    let directory = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&directory.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("账户", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "92233720368547758.06", "大额").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "0.01", "小额").unwrap();
    ledger.set_deposits_deleted(&selection(&ledger.snapshot().unwrap().entries), true).unwrap();
    let deleted = selection(&ledger.snapshot().unwrap().entries);
    ledger.record_deposit(&account, "2026-10-25", "0.01", "新增").unwrap();
    let before = ledger.snapshot().unwrap();
    assert!(ledger.set_deposits_deleted(&deleted, false).is_err());
    assert_eq!(ledger.snapshot().unwrap().accounts[0].balance, "0.01");
    assert_eq!(ledger.snapshot().unwrap().entries, before.entries);
    for entry in deleted { assert_eq!(ledger.entry_history(&entry.id).unwrap().len(), 2); }
}
