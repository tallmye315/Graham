use crate::AppLedger;
use std::path::Path;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;
use graham_ledger::{BackupPreview, BackupRestored, Ledger};

#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupFileAction { Export, Restore }

#[tauri::command]
pub async fn choose_backup_file(app: tauri::AppHandle, action: BackupFileAction) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let window = app.get_webview_window("main").ok_or("找不到 Graham 主窗口")?;
        let picker = app.dialog().file().set_parent(&window).add_filter("Graham 加密备份", &["grahambak"]);
        let selected = match action {
            BackupFileAction::Export => {
                let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| "系统时间异常")?.as_secs();
                picker.set_title("保存 Graham 加密备份").set_file_name(format!("Graham-{seconds}.grahambak")).blocking_save_file()
            }
            BackupFileAction::Restore => picker.set_title("选择 Graham 加密备份").blocking_pick_file(),
        };
        selected.map(|file| file.into_path().map(|path| path.to_string_lossy().into_owned()).map_err(|_| "请选择本地备份文件".to_string())).transpose()
    }).await.map_err(|_| "无法打开文件选择窗口，请重试".to_string())?
}

#[tauri::command]
pub async fn export_backup(app: tauri::AppHandle, path: String, password: String) -> Result<(), String> {
    let password = Zeroizing::new(password);
    tauri::async_runtime::spawn_blocking(move || {
        let ledger = app.state::<AppLedger>();
        let mut ledger = ledger.lock().map_err(|_| "账本正在恢复，请重新启动应用".to_string())?;
        ledger.export_backup(Path::new(&path), &password)
    }).await.map_err(|_| "导出未完成，请重试".to_string())?
}

#[tauri::command]
pub async fn preview_backup(path: String, password: String) -> Result<BackupPreview, String> {
    let password = Zeroizing::new(password);
    tauri::async_runtime::spawn_blocking(move || Ledger::preview_backup(Path::new(&path), &password))
        .await.map_err(|_| "验证未完成，当前账本未修改，请重试".to_string())?
}

#[tauri::command]
pub async fn restore_backup(app: tauri::AppHandle, path: String, password: String, fingerprint: String) -> Result<BackupRestored, String> {
    let password = Zeroizing::new(password);
    tauri::async_runtime::spawn_blocking(move || {
        let recovery = app.path().app_local_data_dir().map_err(|_| "无法定位恢复前备份目录")?.join("backups");
        let ledger = app.state::<AppLedger>();
        let mut ledger = ledger.lock().map_err(|_| "账本正在恢复，请重新启动应用".to_string())?;
        ledger.restore_backup(Path::new(&path), &password, &fingerprint, &recovery)
    }).await.map_err(|_| "恢复过程未完成，请重新打开账本核对，恢复前备份仍保留".to_string())?
}
