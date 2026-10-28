use crate::AppRefresh;
use graham_ledger::{MoneyWeightedState, PerformanceQuery, PerformanceState};
use tauri::Manager;

#[tauri::command]
pub async fn get_performance(app: tauri::AppHandle, query: PerformanceQuery) -> Result<PerformanceState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().performance(&query))
        .await.map_err(|_| "投资表现读取未完成".to_string())?
}
#[tauri::command]
pub async fn refresh_performance(app: tauri::AppHandle, query: PerformanceQuery) -> Result<PerformanceState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let refresh = app.state::<AppRefresh>();
        let _task = refresh.start_performance(query.clone())?;
        refresh.performance(&query)
    }).await.map_err(|_| "投资表现刷新未完成".to_string())?
}
#[tauri::command]
pub async fn get_money_weighted(app: tauri::AppHandle, query: PerformanceQuery) -> Result<MoneyWeightedState, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppRefresh>().money_weighted(&query))
        .await.map_err(|_| "个人资金回报读取未完成".to_string())?
}
#[tauri::command]
pub async fn refresh_money_weighted(app: tauri::AppHandle, query: PerformanceQuery) -> Result<MoneyWeightedState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let refresh = app.state::<AppRefresh>();
        let _task = refresh.start_money_weighted(query.clone())?;
        refresh.money_weighted(&query)
    }).await.map_err(|_| "个人资金回报刷新未完成".to_string())?
}
#[tauri::command]
pub fn stop_performance_refresh(app: tauri::AppHandle) -> Result<(), String> {
    app.state::<AppRefresh>().stop_history()
}
