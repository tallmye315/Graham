use crate::AppRefresh;
use graham_ledger::{IncomeQuery, IncomeState};
use tauri::Manager;

#[tauri::command]
pub async fn get_income(app: tauri::AppHandle, query: IncomeQuery) -> Result<IncomeState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().income(&query))
        .await.map_err(|_| "收益读取未完成，请重试".to_string())?
}

#[tauri::command]
pub async fn refresh_income(app: tauri::AppHandle, query: IncomeQuery) -> Result<IncomeState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().refresh_income(&query))
        .await.map_err(|_| "收益刷新未完成，请读取当前缓存".to_string())?
}
