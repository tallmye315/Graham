use crate::AppLedger;
use graham_ledger::{Snapshot, TradeInput, TradeRevision, PositionValue};
use tauri::State;

#[tauri::command]
pub fn record_trade(ledger: State<'_, AppLedger>, input: TradeInput) -> Result<Snapshot, String> {
    ledger.lock().map_err(|_| "账本不可用，请重新启动软件")?.record_trade(&input)
}

#[tauri::command]
pub fn revise_trade(ledger: State<'_, AppLedger>, id: String, expected_revision: i64, input: TradeInput) -> Result<Snapshot, String> {
    ledger.lock().map_err(|_| "账本不可用，请重新启动软件")?.revise_trade(&id, expected_revision, &input)
}

#[tauri::command]
pub fn set_trade_deleted(ledger: State<'_, AppLedger>, id: String, expected_revision: i64, deleted: bool) -> Result<Snapshot, String> {
    ledger.lock().map_err(|_| "账本不可用，请重新启动软件")?.set_trade_deleted(&id, expected_revision, deleted)
}

#[tauri::command]
pub fn permanently_delete_trade(ledger: State<'_, AppLedger>, id: String, expected_revision: i64) -> Result<Snapshot, String> {
    ledger.lock().map_err(|_| "账本不可用，请重新启动软件")?.permanently_delete_trade(&id, expected_revision)
}

#[tauri::command]
pub fn restore_trade_revision(ledger: State<'_, AppLedger>, id: String, expected_revision: i64, restore_revision: i64) -> Result<Snapshot, String> {
    ledger.lock().map_err(|_| "账本不可用，请重新启动软件")?.restore_trade_revision(&id, expected_revision, restore_revision)
}

#[tauri::command]
pub fn trade_history(ledger: State<'_, AppLedger>, id: String) -> Result<Vec<TradeRevision>, String> {
    ledger.lock().map_err(|_| "账本不可用，请重新启动软件")?.trade_history(&id)
}

#[tauri::command]
pub fn value_position(ledger: State<'_, AppLedger>, account: String, market: String, code: String, price: String) -> Result<PositionValue, String> {
    ledger.lock().map_err(|_| "账本不可用，请重新启动软件")?.value_position(&account, &market, &code, &price)
}
