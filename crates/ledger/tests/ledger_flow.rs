use graham_ledger::Ledger;

#[test]
fn recorded_deposit_survives_closing_and_reopening_the_application_ledger() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graham.sqlite3");
    let account_id;
    {
        let mut ledger = Ledger::open(&path).unwrap();
        assert!(ledger.snapshot().unwrap().accounts.is_empty());
        account_id = ledger.create_account("华龙证券", "CNY").unwrap();
        ledger.record_deposit(&account_id, "2026-10-24", "100000.01", "首次入金").unwrap();
    }
    let ledger = Ledger::open(&path).unwrap();
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.accounts.len(), 1);
    assert_eq!(snapshot.accounts[0].name, "华龙证券");
    assert_eq!(snapshot.accounts[0].currency, "CNY");
    assert_eq!(snapshot.accounts[0].balance, "100000.01");
    assert_eq!(snapshot.entries.len(), 1);
    assert_eq!(snapshot.entries[0].account_id, account_id);
    assert_eq!(snapshot.entries[0].amount, "100000.01");
    assert_eq!(snapshot.entries[0].occurred_on, "2026-10-24");
    assert_eq!(snapshot.entries[0].note, "首次入金");
}

#[test]
fn invalid_fraction_does_not_change_the_ledger() {
    let directory = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&directory.path().join("graham.sqlite3")).unwrap();
    let account = ledger.create_account("兴业证券", "CNY").unwrap();
    assert!(ledger.record_deposit(&account, "2026-10-24", "1.001", "").is_err());
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.accounts[0].balance, "0.00");
    assert!(snapshot.entries.is_empty());
}

#[test]
fn invalid_accounts_are_not_created() {
    let directory = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&directory.path().join("graham.sqlite3")).unwrap();
    for (name, currency) in [("  ", "CNY"), ("账户", "BTC")] {
        assert!(ledger.create_account(name, currency).is_err());
    }
    assert!(ledger.snapshot().unwrap().accounts.is_empty());
}

#[test]
fn incoming_deposit_cannot_overflow_the_existing_balance() {
    let directory = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&directory.path().join("graham.sqlite3")).unwrap();
    let account = ledger.create_account("港币账户", "HKD").unwrap();
    ledger.record_deposit(&account, "2026-10-24", "92233720368547758.07", "").unwrap();
    assert!(ledger.record_deposit(&account, "2026-10-24", "0.01", "").is_err());
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.accounts[0].balance, "92233720368547758.07");
    assert_eq!(snapshot.entries.len(), 1);
}

#[test]
fn invalid_deposits_leave_the_existing_account_and_records_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&directory.path().join("graham.sqlite3")).unwrap();
    let account = ledger.create_account("美元账户", "USD").unwrap();
    ledger.record_deposit(&account, "2024-02-29", "12.30", "已核对").unwrap();
    for (date, amount) in [("2026-02-30", "1"), ("2026-2-3", "1"), ("0000-01-01", "1"), ("2026-10-24", "0"), ("2026-10-24", "-1"), ("2026-10-24", "1e3"), ("2026-10-24", "92233720368547758.08")] {
        assert!(ledger.record_deposit(&account, date, amount, "").is_err());
    }
    assert!(ledger.record_deposit("不存在的账户", "2026-10-24", "1", "").is_err());
    assert!(ledger.record_deposit(&account, "2026-10-24", "1", &"字".repeat(501)).is_err());
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.accounts[0].balance, "12.30");
    assert_eq!(snapshot.entries.len(), 1);
    assert_eq!(snapshot.entries[0].note, "已核对");
}

#[test]
fn decimal_deposits_stay_exact_and_separate_between_accounts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graham.sqlite3");
    let mut ledger = Ledger::open(&path).unwrap();
    let first = ledger.create_account("华龙证券", "CNY").unwrap();
    let second = ledger.create_account("海外账户", "USD").unwrap();
    ledger.record_deposit(&first, "2026-10-24", "0.10", "").unwrap();
    ledger.record_deposit(&first, "2026-10-24", "0.20", "").unwrap();
    ledger.record_deposit(&second, "2026-10-24", "0.01", "").unwrap();
    drop(ledger);
    let snapshot = Ledger::open(&path).unwrap().snapshot().unwrap();
    assert_eq!(snapshot.accounts[0].balance, "0.30");
    assert_eq!(snapshot.accounts[1].balance, "0.01");
    assert_eq!(snapshot.entries.len(), 3);
    assert!(snapshot.entries.iter().all(|entry| entry.source == "手动录入"));
}
