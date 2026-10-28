use crate::AppLedger;
use graham_ledger::{DividendInput,DividendRevision,Snapshot};
use tauri::State;
#[tauri::command]
pub fn record_dividend(ledger:State<'_,AppLedger>,input:DividendInput)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.record_dividend(&input)}
#[tauri::command]
pub fn revise_dividend(ledger:State<'_,AppLedger>,id:String,expected_revision:i64,input:DividendInput)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.revise_dividend(&id,expected_revision,&input)}
#[tauri::command]
pub fn set_dividend_deleted(ledger:State<'_,AppLedger>,id:String,expected_revision:i64,deleted:bool)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.set_dividend_deleted(&id,expected_revision,deleted)}
#[tauri::command]
pub fn restore_dividend_revision(ledger:State<'_,AppLedger>,id:String,expected_revision:i64,restore_revision:i64)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.restore_dividend_revision(&id,expected_revision,restore_revision)}
#[tauri::command]
pub fn permanently_delete_dividend(ledger:State<'_,AppLedger>,id:String,expected_revision:i64)->Result<Snapshot,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.permanently_delete_dividend(&id,expected_revision)}
#[tauri::command]
pub fn dividend_history(ledger:State<'_,AppLedger>,id:String)->Result<Vec<DividendRevision>,String>{ledger.lock().map_err(|_|"账本不可用，请重启")?.dividend_history(&id)}
