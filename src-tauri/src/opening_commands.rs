use crate::AppLedger;
use graham_ledger::{OpeningInput, OpeningRevision, Snapshot, DatedSnapshot};
use tauri::State;

#[tauri::command]
pub fn save_opening(ledger:State<'_,AppLedger>,input:OpeningInput,expected_revision:Option<i64>)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.save_opening(&input,expected_revision)}
#[tauri::command]
pub fn set_opening_deleted(ledger:State<'_,AppLedger>,id:String,expected_revision:i64,deleted:bool)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.set_opening_deleted(&id,expected_revision,deleted)}
#[tauri::command]
pub fn restore_opening_revision(ledger:State<'_,AppLedger>,id:String,expected_revision:i64,restore_revision:i64)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.restore_opening_revision(&id,expected_revision,restore_revision)}
#[tauri::command]
pub fn permanently_delete_opening(ledger:State<'_,AppLedger>,id:String,expected_revision:i64)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.permanently_delete_opening(&id,expected_revision)}
#[tauri::command]
pub fn opening_history(ledger:State<'_,AppLedger>,id:String)->Result<Vec<OpeningRevision>,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.opening_history(&id)}
#[tauri::command]
pub fn snapshot_on(ledger:State<'_,AppLedger>,date:String)->Result<DatedSnapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.snapshot_on(&date)}
