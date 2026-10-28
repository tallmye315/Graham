use std::{os::windows::process::CommandExt, path::PathBuf, process::Command};

pub fn repair() -> Result<(), String> {
    let executable = dunce::canonicalize(std::env::current_exe().map_err(|_| "找不到软件安装位置")?)
        .map_err(|_| "无法解析软件的实际安装位置")?;
    let directory = executable.parent().ok_or("找不到软件安装目录")?;
    let icon = dunce::canonicalize(directory.join("graham-icon.ico"))
        .map_err(|_| "请从安装包安装的软件中创建快捷方式")?;
    let powershell = PathBuf::from(std::env::var_os("SystemRoot").ok_or("无法定位系统目录")?)
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let output = Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", include_str!("shortcuts.ps1")])
        .env("GRAHAM_SHORTCUT_TARGET", &executable)
        .env("GRAHAM_SHORTCUT_DIRECTORY", directory)
        .env("GRAHAM_SHORTCUT_ICON", icon)
        .creation_flags(0x08000000)
        .output().map_err(|_| "创建快捷方式失败，请重试")?;
    if !output.status.success() {
        eprintln!("Graham shortcut creation failed: {}", String::from_utf8_lossy(&output.stderr));
        return Err("无法写入快捷方式，请检查桌面和开始菜单目录权限".into());
    }
    Ok(())
}
