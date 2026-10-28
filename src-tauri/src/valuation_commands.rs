use crate::AppRefresh;
use graham_ledger::{AnalysisState, ValuationQuery, ValuationState};
use tauri::Manager;

#[tauri::command]
pub async fn get_valuation(app: tauri::AppHandle, query: ValuationQuery) -> Result<ValuationState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().valuation(&query))
        .await.map_err(|_| "估值读取未完成".to_string())?
}
#[tauri::command]
pub async fn refresh_valuation(app: tauri::AppHandle, query: ValuationQuery, probe: bool) -> Result<ValuationState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().refresh_valuation(&query, probe))
        .await.map_err(|_| "估值刷新未完成，请读取缓存".to_string())?
}
#[tauri::command]
pub async fn get_analysis(app: tauri::AppHandle, query: ValuationQuery) -> Result<AnalysisState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().analysis(&query))
        .await.map_err(|_| "组合分析读取未完成".to_string())?
}
#[tauri::command]
pub async fn refresh_analysis(app: tauri::AppHandle, query: ValuationQuery) -> Result<AnalysisState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().refresh_analysis(&query))
        .await.map_err(|_| "组合分析刷新未完成，请读取缓存".to_string())?
}
