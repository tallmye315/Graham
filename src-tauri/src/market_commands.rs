use crate::AppRefresh;
use graham_ledger::QuoteState;
use tauri::Manager;

#[tauri::command]
pub async fn get_market_quotes(app: tauri::AppHandle, probe: bool) -> Result<QuoteState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().quotes(probe))
        .await.map_err(|_| "读取行情未完成".to_string())?
}
#[tauri::command]
pub async fn refresh_market_quotes(app: tauri::AppHandle, probe: bool) -> Result<QuoteState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().refresh_quotes(probe))
        .await.map_err(|_| "刷新行情未完成，请读取当前缓存".to_string())?
}
#[tauri::command]
pub async fn set_market_auto(app: tauri::AppHandle, enabled: bool) -> Result<QuoteState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().set_automatic(enabled))
        .await.map_err(|_| "更新自动刷新未完成".to_string())?
}
