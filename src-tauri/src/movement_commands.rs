use crate::AppLedger;
use graham_ledger::{MovementInput,MovementRevision,FlowSummary,Snapshot};
use tauri::State;
#[tauri::command]
pub fn record_movement(ledger:State<'_,AppLedger>,input:MovementInput)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.record_movement(&input)}
#[tauri::command]
pub fn revise_movement(ledger:State<'_,AppLedger>,id:String,expected_revision:i64,input:MovementInput)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.revise_movement(&id,expected_revision,&input)}
#[tauri::command]
pub fn set_movement_deleted(ledger:State<'_,AppLedger>,id:String,expected_revision:i64,deleted:bool)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.set_movement_deleted(&id,expected_revision,deleted)}
#[tauri::command]
pub fn restore_movement_revision(ledger:State<'_,AppLedger>,id:String,expected_revision:i64,restore_revision:i64)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.restore_movement_revision(&id,expected_revision,restore_revision)}
#[tauri::command]
pub fn permanently_delete_movement(ledger:State<'_,AppLedger>,id:String,expected_revision:i64)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.permanently_delete_movement(&id,expected_revision)}
#[tauri::command]
pub fn movement_history(ledger:State<'_,AppLedger>,id:String)->Result<Vec<MovementRevision>,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.movement_history(&id)}
#[tauri::command]
pub fn cash_flows(ledger:State<'_,AppLedger>,account_ids:Vec<String>,start_on:String,end_on:String)->Result<FlowSummary,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.cash_flows(&account_ids,&start_on,&end_on)}
