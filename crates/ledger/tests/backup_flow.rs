mod support;
use support::deposit_revision;
use graham_ledger::{Entry, Ledger};
use std::path::Path;

const PASSWORD: &str = "验收用备份口令-2026-only";

fn assert_same_records(actual: &[Entry], expected: &[Entry]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!((&actual.id, &actual.account_id, &actual.amount, &actual.occurred_on, &actual.note, &actual.source, actual.deleted), (&expected.id, &expected.account_id, &expected.amount, &expected.occurred_on, &expected.note, &expected.source, expected.deleted));
        assert!(actual.revision_id >= expected.revision_id);
    }
}

#[test]
fn encrypted_backup_restores_records_history_and_account_settings_and_keeps_pre_restore_copy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let backup = directory.path().join("portfolio.grahambak");
    let recovery = directory.path().join("recovery");
    let mut ledger = Ledger::open(&path).unwrap();
    let first = ledger.create_account("人民币验收账户", "CNY").unwrap();
    let second = ledger.create_account("美元验收账户", "USD").unwrap();
    ledger.record_deposit(&first, "2026-10-25", "12.30", "private-note-that-must-be-encrypted").unwrap();
    let id = ledger.snapshot().unwrap().entries[0].id.clone();
    ledger.revise_deposit(&id, deposit_revision(&ledger, &id), "2026-10-25", "8.90", "修正").unwrap();
    ledger.set_deposit_deleted(&id, deposit_revision(&ledger, &id), true).unwrap();
    ledger.record_deposit(&second, "2026-10-25", "7.70", "有效记录").unwrap();
    ledger.set_account_archived(&first, true).unwrap();
    let expected = ledger.snapshot().unwrap();
    ledger.export_backup(&backup, PASSWORD).unwrap();
    let bytes = std::fs::read(&backup).unwrap();
    assert!(bytes.starts_with(b"age-encryption.org/v1"));
    assert!(!bytes.windows(35).any(|w| w == b"private-note-that-must-be-encrypted"));
    let preview = Ledger::preview_backup(&backup, PASSWORD).unwrap();
    assert_eq!(preview.account_count, 2);
    assert_eq!(preview.record_count, 2);
    assert_eq!(preview.deleted_count, 1);

    ledger.record_deposit(&second, "2026-10-25", "2.00", "备份之后的记录").unwrap();
    let before_restore = ledger.snapshot().unwrap();
    let restored = ledger.restore_backup(&backup, PASSWORD, &preview.fingerprint, &recovery).unwrap();
    assert_eq!(restored.snapshot.accounts, expected.accounts);
    assert_same_records(&restored.snapshot.entries, &expected.entries);
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.snapshot().unwrap().accounts, expected.accounts);
    assert_same_records(&ledger.snapshot().unwrap().entries, &expected.entries);
    let history = ledger.entry_history(&id).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[2].note, "private-note-that-must-be-encrypted");

    let saved = Path::new(&restored.recovery_path);
    let saved_preview = Ledger::preview_backup(saved, PASSWORD).unwrap();
    let mut other = Ledger::open(&directory.path().join("other.sqlite3")).unwrap();
    other.restore_backup(saved, PASSWORD, &saved_preview.fingerprint, &directory.path().join("other-recovery")).unwrap();
    assert_eq!(other.snapshot().unwrap().accounts, before_restore.accounts);
    assert_same_records(&other.snapshot().unwrap().entries, &before_restore.entries);
}

#[test]
fn failed_or_changed_backups_and_unwritable_recovery_location_leave_the_current_ledger_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&directory.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("当前账户", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "10", "原记录").unwrap();
    let backup = directory.path().join("first.grahambak");
    ledger.export_backup(&backup, PASSWORD).unwrap();
    let preview = Ledger::preview_backup(&backup, PASSWORD).unwrap();
    let encrypted = std::fs::read(&backup).unwrap();
    assert!(ledger.export_backup(&backup, PASSWORD).is_err());
    assert_eq!(std::fs::read(&backup).unwrap(), encrypted);
    let short = directory.path().join("short.grahambak");
    assert!(ledger.export_backup(&short, "short").is_err());
    assert!(!short.exists());

    ledger.record_deposit(&account, "2026-10-25", "5", "后来录入").unwrap();
    let before = ledger.snapshot().unwrap();
    let histories: Vec<_> = before.entries.iter().map(|e| ledger.entry_history(&e.id).unwrap().len()).collect();
    let recovery = directory.path().join("recovery");
    assert!(ledger.restore_backup(&backup, "错误的密码", &preview.fingerprint, &recovery).is_err());
    let broken = directory.path().join("broken.grahambak");
    std::fs::write(&broken, &encrypted[..encrypted.len()-8]).unwrap();
    assert!(ledger.restore_backup(&broken, PASSWORD, &preview.fingerprint, &recovery).is_err());
    assert!(!recovery.exists());

    let changed = directory.path().join("changed.grahambak");
    ledger.export_backup(&changed, PASSWORD).unwrap();
    assert!(ledger.restore_backup(&changed, PASSWORD, &preview.fingerprint, &recovery).unwrap_err().contains("已变化"));
    let blocked = directory.path().join("not-a-directory");
    std::fs::write(&blocked, b"preserve this file").unwrap();
    assert!(ledger.restore_backup(&backup, PASSWORD, &preview.fingerprint, &blocked).is_err());
    assert_eq!(std::fs::read(&blocked).unwrap(), b"preserve this file");
    drop(ledger);
    let ledger = Ledger::open(&directory.path().join("ledger.sqlite3")).unwrap();
    assert_eq!(ledger.snapshot().unwrap().accounts, before.accounts);
    assert_eq!(ledger.snapshot().unwrap().entries, before.entries);
    for (entry, count) in before.entries.iter().zip(histories) { assert_eq!(ledger.entry_history(&entry.id).unwrap().len(), count); }
}

#[test]
fn unsupported_versions_and_inconsistent_authenticated_contents_do_not_replace_the_ledger() {
    use age::secrecy::SecretString;
    let directory = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&directory.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("当前账户", "USD").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "7.70", "当前备注").unwrap();
    let before = ledger.snapshot().unwrap();
    let backup = directory.path().join("valid.grahambak");
    ledger.export_backup(&backup, PASSWORD).unwrap();
    let encrypted = std::fs::read(&backup).unwrap();
    let mut identity = age::scrypt::Identity::new(SecretString::from(PASSWORD.to_owned()));
    identity.set_max_work_factor(18);
    let plaintext = age::decrypt(&identity, &encrypted).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
    for invalid_version in [true, false] {
        let mut altered = document.clone();
        if invalid_version { altered["version"] = serde_json::json!(99); }
        else { altered["deposits"][0]["note"] = serde_json::json!("与最新历史不一致"); }
        let mut recipient = age::scrypt::Recipient::new(SecretString::from(PASSWORD.to_owned()));
        recipient.set_work_factor(18);
        let bytes = age::encrypt(&recipient, &serde_json::to_vec(&altered).unwrap()).unwrap();
        let forged = directory.path().join(format!("invalid-{invalid_version}.grahambak"));
        std::fs::write(&forged, bytes).unwrap();
        let error = ledger.restore_backup(&forged, PASSWORD, "irrelevant", &directory.path().join("recovery")).unwrap_err();
        assert!(error.contains(if invalid_version { "版本不受支持" } else { "损坏或不完整" }));
        assert_eq!(ledger.snapshot().unwrap().accounts, before.accounts);
        assert_eq!(ledger.snapshot().unwrap().entries, before.entries);
    }
}

#[test]
fn restoring_invalidates_selections_and_history_handles_held_by_another_window() {
    use graham_ledger::RecordSelection;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let account = ledger.create_account("账户", "CNY").unwrap();
    ledger.record_deposit(&account, "2026-10-25", "10", "备份值").unwrap();
    let mut other_window = Ledger::open(&path).unwrap();
    let old = other_window.snapshot().unwrap().entries.remove(0);
    let selection = [RecordSelection { id: old.id.clone(), revision_id: old.revision_id }];
    let backup = directory.path().join("original.grahambak");
    ledger.export_backup(&backup, PASSWORD).unwrap();
    let preview = Ledger::preview_backup(&backup, PASSWORD).unwrap();
    ledger.revise_deposit(&old.id, deposit_revision(&ledger, &old.id), "2026-10-25", "20", "后来修改").unwrap();
    ledger.restore_backup(&backup, PASSWORD, &preview.fingerprint, &directory.path().join("recovery")).unwrap();
    assert!(other_window.set_deposits_deleted(&selection, true).is_err());
    assert!(other_window.restore_deposit_revision(&old.id, old.revision_id, old.revision_id).is_err());
    assert_eq!(other_window.snapshot().unwrap().accounts[0].balance, "10.00");
    assert!(other_window.snapshot().unwrap().entries[0].revision_id > old.revision_id);
}
