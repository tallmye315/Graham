use graham_ledger::Ledger;

pub fn deposit_revision(ledger: &Ledger, id: &str) -> i64 {
    ledger.snapshot().unwrap().entries.iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.revision_id)
        .unwrap_or(0)
}
