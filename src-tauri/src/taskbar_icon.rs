use std::io;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CopyIcon, DestroyIcon, GetClassLongPtrW, SendMessageW, GCLP_HICONSM,
    ICON_BIG, ICON_SMALL, WM_GETICON, WM_SETICON,
};

// Tauri owns the title-bar icon. Own a copy so its replacement cannot invalidate
// the taskbar icon. App state keeps this resource alive until shutdown.
pub struct TaskbarIcon(usize);

impl TaskbarIcon {
    pub fn install(window: &tauri::WebviewWindow) -> Result<Self, Box<dyn std::error::Error>> {
        let hwnd = window.hwnd()?.0;
        // SAFETY: hwnd is the live Tauri main window, called on its setup thread.
        // CopyIcon creates an independent owned handle; it is freed in Drop.
        unsafe {
            let mut small = SendMessageW(hwnd, WM_GETICON, ICON_SMALL as usize, 0);
            if small == 0 { small = GetClassLongPtrW(hwnd, GCLP_HICONSM) as isize; }
            if small == 0 { return Err(io::Error::other("Graham 窗口品牌图标尚未初始化").into()); }
            let icon = CopyIcon(small as _);
            if icon.is_null() { return Err(io::Error::last_os_error().into()); }
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon as isize);
            Ok(Self(icon as usize))
        }
    }
}

impl Drop for TaskbarIcon {
    fn drop(&mut self) {
        // SAFETY: this handle was created by CopyIcon and has a single owner.
        unsafe { DestroyIcon(self.0 as _); }
    }
}
