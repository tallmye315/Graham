use crate::AppLedger;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use graham_ledger::{ImportLog,CsvPreview,ImportResult,BatchUndoPreview};
#[tauri::command]
pub async fn import_log(app:tauri::AppHandle)->Result<ImportLog,String>{
 tauri::async_runtime::spawn_blocking(move||{let state=app.state::<AppLedger>();let mut ledger=state.lock().map_err(|_|"账本不可用，请重启")?;ledger.import_log()}).await.map_err(|_|"操作未完成，请刷新账本核对".to_string())?
}
#[tauri::command]
pub async fn preview_csv(app:tauri::AppHandle,text:String)->Result<CsvPreview,String>{
 tauri::async_runtime::spawn_blocking(move||{let state=app.state::<AppLedger>();let mut ledger=state.lock().map_err(|_|"账本不可用，请重启")?;ledger.preview_csv(&text)}).await.map_err(|_|"操作未完成，请刷新账本核对".to_string())?
}
#[tauri::command]
pub async fn commit_csv(app:tauri::AppHandle,text:String,expected_fingerprint:String)->Result<ImportResult,String>{
 tauri::async_runtime::spawn_blocking(move||{let state=app.state::<AppLedger>();let mut ledger=state.lock().map_err(|_|"账本不可用，请重启")?;ledger.commit_csv(&text,&expected_fingerprint)}).await.map_err(|_|"操作未完成，请刷新账本核对".to_string())?
}
#[tauri::command]
pub async fn revise_import_item(app:tauri::AppHandle,id:String,text:String,expected_fingerprint:String)->Result<ImportResult,String>{
 tauri::async_runtime::spawn_blocking(move||{let state=app.state::<AppLedger>();let mut ledger=state.lock().map_err(|_|"账本不可用，请重启")?;ledger.revise_import_item(&id,&text,&expected_fingerprint)}).await.map_err(|_|"操作未完成，请刷新账本核对".to_string())?
}
#[tauri::command]
pub async fn set_import_item_deleted(app:tauri::AppHandle,id:String,expected_fingerprint:String,deleted:bool)->Result<ImportResult,String>{
 tauri::async_runtime::spawn_blocking(move||{let state=app.state::<AppLedger>();let mut ledger=state.lock().map_err(|_|"账本不可用，请重启")?;ledger.set_import_item_deleted(&id,&expected_fingerprint,deleted)}).await.map_err(|_|"操作未完成，请刷新账本核对".to_string())?
}
#[tauri::command]
pub async fn preview_import_batch_undo(app:tauri::AppHandle,batch_id:String)->Result<BatchUndoPreview,String>{
 tauri::async_runtime::spawn_blocking(move||{let state=app.state::<AppLedger>();let mut ledger=state.lock().map_err(|_|"账本不可用，请重启")?;ledger.preview_import_batch_undo(&batch_id)}).await.map_err(|_|"操作未完成，请刷新账本核对".to_string())?
}
#[tauri::command]
pub async fn undo_import_batch(app:tauri::AppHandle,batch_id:String,expected_fingerprint:String)->Result<ImportResult,String>{
 tauri::async_runtime::spawn_blocking(move||{let state=app.state::<AppLedger>();let mut ledger=state.lock().map_err(|_|"账本不可用，请重启")?;ledger.undo_import_batch(&batch_id,&expected_fingerprint)}).await.map_err(|_|"操作未完成，请刷新账本核对".to_string())?
}
#[tauri::command]
pub async fn choose_csv_file(app:tauri::AppHandle)->Result<Option<String>,String>{
 tauri::async_runtime::spawn_blocking(move||{
  use std::io::Read;let window=app.get_webview_window("main").ok_or("找不到主窗口")?;
  let selected=app.dialog().file().set_parent(&window).add_filter("UTF-8 CSV",&["csv"]).set_title("选择统一格式 CSV").blocking_pick_file();
  let Some(file)=selected else{return Ok(None);};let path=file.into_path().map_err(|_|"请选择本地文件")?;let mut bytes=Vec::new();std::fs::File::open(path).map_err(|_|"无法读取 CSV")?.take(2*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"CSV 读取失败")?;if bytes.len()>2*1024*1024{return Err("CSV 超过 2 MiB 上限".into());}Ok(Some(String::from_utf8(bytes).map_err(|_|"请先将文件另存为 UTF-8 CSV")?))
 }).await.map_err(|_|"文件选择未完成".to_string())?
}
