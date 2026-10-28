#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use graham_ledger::{Ledger, Snapshot, Revision, RecordSelection, RecordRestore};
use std::sync::{Arc, Mutex};
use graham_ledger::market_refresh::{MarketRefresh, SystemClock};
use tauri::{Manager, State};

type AppLedger = Arc<Mutex<Ledger>>;
type AppRefresh = Arc<MarketRefresh>;

mod backup_commands;
mod trade_commands;
mod opening_commands;
mod movement_commands;
mod dividend_commands;
mod import_commands;
mod market_commands;
mod market_process;
mod valuation_commands;
mod performance_commands;
mod income_commands;

#[cfg(target_os = "windows")]
mod taskbar_icon;

#[cfg(target_os = "windows")]
mod shortcuts;

#[tauri::command]
fn ledger_snapshot(ledger: State<'_, AppLedger>) -> Result<Snapshot, String> {
    ledger.lock().map_err(|_| "账本正在恢复，请重新启动应用".to_string())?.snapshot()
}

#[tauri::command]
fn create_account(ledger: State<'_, AppLedger>, name: String, currency: String) -> Result<Snapshot, String> {
    let mut ledger = ledger.lock().map_err(|_| "账本正在恢复，请重新启动应用".to_string())?;
    ledger.create_account(&name, &currency)?;
    ledger.snapshot()
}

#[tauri::command]
fn record_deposit(ledger: State<'_, AppLedger>, account_id: String, date: String, amount: String, note: String) -> Result<Snapshot, String> {
    let mut ledger = ledger.lock().map_err(|_| "账本正在恢复，请重新启动应用".to_string())?;
    ledger.record_deposit(&account_id, &date, &amount, &note)?;
    ledger.snapshot()
}

fn change_ledger(ledger: State<'_, AppLedger>, change: impl FnOnce(&mut Ledger) -> Result<(), String>) -> Result<Snapshot, String> {
    let mut ledger = ledger.lock().map_err(|_| "账本正在恢复，请重新启动应用".to_string())?;
    change(&mut ledger)?;
    ledger.snapshot()
}

#[tauri::command]
fn revise_deposit(ledger: State<'_, AppLedger>, id: String, expected_revision: i64, date: String, amount: String, note: String) -> Result<Snapshot, String> {
    ledger.lock().map_err(|_| "账本不可用".to_string())?.revise_deposit(&id, expected_revision, &date, &amount, &note)
}

#[tauri::command]
fn set_deposit_deleted(ledger: State<'_, AppLedger>, id: String, expected_revision: i64, deleted: bool) -> Result<Snapshot, String> {
    ledger.lock().map_err(|_| "账本不可用".to_string())?.set_deposit_deleted(&id, expected_revision, deleted)
}

#[tauri::command]
fn permanently_delete_deposits(ledger: State<'_, AppLedger>, selection: Vec<RecordSelection>) -> Result<Snapshot, String> {
    change_ledger(ledger, |l| l.permanently_delete_deposits(&selection))
}

#[derive(serde::Serialize)]
struct BatchUpdate { snapshot: Snapshot, undo: Vec<RecordRestore> }

#[tauri::command]
fn set_deposits_deleted(ledger: State<'_, AppLedger>, selection: Vec<RecordSelection>, deleted: bool) -> Result<BatchUpdate, String> {
    let mut ledger = ledger.lock().map_err(|_| "账本正在恢复，请重新启动应用".to_string())?;
    let undo = ledger.set_deposits_deleted(&selection, deleted)?;
    Ok(BatchUpdate { snapshot: ledger.snapshot()?, undo })
}

#[tauri::command]
fn undo_deposit_batch(ledger: State<'_, AppLedger>, restore: Vec<RecordRestore>) -> Result<Snapshot, String> {
    change_ledger(ledger, |l| l.undo_deposit_batch(&restore))
}

#[tauri::command]
fn restore_deposit_revision(ledger: State<'_, AppLedger>, id: String, expected_revision: i64, revision_id: i64) -> Result<Snapshot, String> {
    ledger.lock().map_err(|_| "账本不可用".to_string())?.restore_deposit_revision(&id, expected_revision, revision_id)
}

#[tauri::command]
fn entry_history(ledger: State<'_, AppLedger>, id: String) -> Result<Vec<Revision>, String> {
    ledger.lock().map_err(|_| "账本正在恢复，请重新启动应用".to_string())?.entry_history(&id)
}

#[tauri::command]
fn update_account(ledger: State<'_, AppLedger>, id: String, name: String, currency: String) -> Result<Snapshot, String> {
    change_ledger(ledger, |l| l.update_account(&id, &name, &currency))
}

#[tauri::command]
fn delete_empty_account(ledger: State<'_, AppLedger>, id: String) -> Result<Snapshot, String> {
    change_ledger(ledger, |l| l.delete_empty_account(&id))
}

#[tauri::command]
fn set_account_archived(ledger: State<'_, AppLedger>, id: String, archived: bool) -> Result<Snapshot, String> {
    change_ledger(ledger, |l| l.set_account_archived(&id, archived))
}

#[tauri::command]
fn create_desktop_shortcut() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        shortcuts::repair()?;
        Ok("已在桌面创建 Graham 快捷方式".into())
    }
    #[cfg(not(target_os = "windows"))]
    { Err("桌面快捷方式仅适用于 Windows 安装版".into()) }
}

fn main() {
    #[cfg(target_os = "windows")]
    if std::env::args_os().any(|arg| arg == "--repair-shortcuts") {
        match shortcuts::repair() {
            Ok(()) => return,
            Err(error) => { eprintln!("{error}"); std::process::exit(1); }
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(target_os = "windows")]
            {
                let window = app.get_webview_window("main").ok_or_else(|| std::io::Error::other("找不到 Graham 主窗口"))?;
                app.manage(taskbar_icon::TaskbarIcon::install(&window)?);
            }
            let directory = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let ledger = Ledger::open(&directory.join("ledger.sqlite3")).map_err(std::io::Error::other)?;
            let ledger = Arc::new(Mutex::new(ledger));
            let refresh = Arc::new(MarketRefresh::new(ledger.clone(),
                Arc::new(market_process::BundledMarketProcess { resource: app.path().resource_dir()? }),
                Arc::new(SystemClock)));
            app.manage(ledger);
            app.manage(refresh.clone());
            refresh.start_background();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![income_commands::get_income,income_commands::refresh_income,valuation_commands::get_analysis,valuation_commands::refresh_analysis,performance_commands::get_money_weighted,performance_commands::refresh_money_weighted,performance_commands::get_performance,performance_commands::refresh_performance,performance_commands::stop_performance_refresh,valuation_commands::get_valuation,valuation_commands::refresh_valuation,market_commands::get_market_quotes,market_commands::refresh_market_quotes,market_commands::set_market_auto,import_commands::import_log,import_commands::preview_csv,import_commands::commit_csv,import_commands::revise_import_item,import_commands::set_import_item_deleted,import_commands::preview_import_batch_undo,import_commands::undo_import_batch,import_commands::choose_csv_file,dividend_commands::record_dividend,dividend_commands::revise_dividend,dividend_commands::set_dividend_deleted,dividend_commands::restore_dividend_revision,dividend_commands::permanently_delete_dividend,dividend_commands::dividend_history,movement_commands::record_movement,movement_commands::revise_movement,movement_commands::set_movement_deleted,movement_commands::restore_movement_revision,movement_commands::permanently_delete_movement,movement_commands::movement_history,movement_commands::cash_flows,opening_commands::save_opening,opening_commands::set_opening_deleted,opening_commands::restore_opening_revision,opening_commands::permanently_delete_opening,opening_commands::opening_history,opening_commands::snapshot_on,trade_commands::permanently_delete_trade, trade_commands::record_trade, trade_commands::revise_trade, trade_commands::set_trade_deleted, trade_commands::restore_trade_revision, trade_commands::trade_history, trade_commands::value_position, backup_commands::choose_backup_file, backup_commands::export_backup, backup_commands::preview_backup, backup_commands::restore_backup, ledger_snapshot, create_account, record_deposit, revise_deposit, set_deposit_deleted, permanently_delete_deposits, set_deposits_deleted, undo_deposit_batch, restore_deposit_revision, entry_history, update_account, delete_empty_account, set_account_archived, create_desktop_shortcut])
        .run(tauri::generate_context!())
        .expect("Graham 无法启动，请保留账本并检查应用数据目录权限");
}
