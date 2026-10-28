"""Verify Graham's actual window/taskbar or desktop shortcut icon on Windows."""
import argparse
import ctypes as c
from ctypes import wintypes as w
import json
import os
import subprocess
import time
from pathlib import Path
import winreg


class ShellInfo(c.Structure):
    _fields_ = [("icon", w.HICON), ("index", c.c_int), ("attributes", w.DWORD),
                ("name", w.WCHAR * 260), ("type", w.WCHAR * 80)]


class BitmapHeader(c.Structure):
    _fields_ = [("size", w.DWORD), ("width", w.LONG), ("height", w.LONG),
                ("planes", w.WORD), ("bits", w.WORD), ("compression", w.DWORD),
                ("image_size", w.DWORD), ("x", w.LONG), ("y", w.LONG),
                ("colors", w.DWORD), ("important", w.DWORD)]


def desktop_path():
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER,
                       r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders") as key:
        return Path(os.path.expandvars(winreg.QueryValueEx(key, "Desktop")[0]))


def application_window(process_id):
    user = c.WinDLL("user32", use_last_error=True)
    user.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
    user.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
    found = []
    callback_type = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)

    @callback_type
    def visit(handle, _):
        pid = w.DWORD()
        user.GetWindowThreadProcessId(handle, c.byref(pid))
        title = c.create_unicode_buffer(260)
        user.GetWindowTextW(handle, title, 260)
        if pid.value == process_id and title.value == "Graham":
            found.append(handle)
        return True

    user.EnumWindows.argtypes = [callback_type, w.LPARAM]
    user.EnumWindows(visit, 0)
    if len(found) != 1:
        raise RuntimeError(f"Expected one Graham application window, found {len(found)}")
    return found[0]


def verify_shortcut_paths(path):
    script = """$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new();
$link=(New-Object -ComObject WScript.Shell).CreateShortcut($env:GRAHAM_VERIFY_SHORTCUT);
[pscustomobject]@{target=$link.TargetPath;icon=$link.IconLocation;directory=$link.WorkingDirectory}|ConvertTo-Json -Compress"""
    environment = dict(os.environ, GRAHAM_VERIFY_SHORTCUT=str(path))
    output = subprocess.run(
        [str(Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"),
         "-NoProfile", "-NonInteractive", "-Command", script],
        env=environment, capture_output=True, text=True, encoding="utf-8", check=True,
        creationflags=subprocess.CREATE_NO_WINDOW, timeout=15,
    )
    link = json.loads(output.stdout.lstrip("\ufeff"))
    target = Path(link["target"])
    icon = Path(link["icon"].rsplit(",", 1)[0].strip('"'))
    def same_path(first, second):
        return os.path.normcase(os.path.normpath(str(first))) == os.path.normcase(os.path.normpath(str(second)))
    result = {
        "targetExists": target.is_file(),
        "iconExists": icon.is_file(),
        "targetUsesPhysicalPath": same_path(target, target.resolve()),
        "iconUsesPhysicalPath": same_path(icon, icon.resolve()),
        "iconSharesTargetDirectory": same_path(icon, target.parent / "graham-icon.ico"),
        "workingDirectoryMatchesTarget": same_path(link["directory"], target.parent),
    }
    if not all(result.values()):
        raise RuntimeError("Shortcut contains redirected or inconsistent paths: " + json.dumps(result))


def verify(path, window_handle=None, small=False):
    shell = c.WinDLL("shell32", use_last_error=True)
    user = c.WinDLL("user32", use_last_error=True)
    gdi = c.WinDLL("gdi32", use_last_error=True)
    shell.SHGetFileInfoW.argtypes = [w.LPCWSTR, w.DWORD, c.POINTER(ShellInfo), w.UINT, w.UINT]
    shell.SHGetFileInfoW.restype = c.c_size_t
    gdi.CreateCompatibleDC.argtypes = [w.HDC]
    gdi.CreateCompatibleDC.restype = w.HDC
    gdi.CreateDIBSection.argtypes = [w.HDC, c.POINTER(BitmapHeader), w.UINT, c.POINTER(c.c_void_p), w.HANDLE, w.DWORD]
    gdi.CreateDIBSection.restype = w.HBITMAP
    gdi.SelectObject.argtypes = [w.HDC, w.HANDLE]
    gdi.SelectObject.restype = w.HANDLE
    gdi.DeleteObject.argtypes = [w.HANDLE]
    gdi.DeleteDC.argtypes = [w.HDC]
    user.DrawIconEx.argtypes = [w.HDC, c.c_int, c.c_int, w.HICON, c.c_int, c.c_int, w.UINT, w.HBRUSH, w.UINT]
    user.DestroyIcon.argtypes = [w.HICON]
    info = ShellInfo()
    if window_handle:
        user.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, c.c_size_t, c.c_ssize_t, w.UINT, w.UINT, c.POINTER(c.c_size_t)]
        user.SendMessageTimeoutW.restype = c.c_size_t
        user.GetClassLongPtrW.argtypes = [w.HWND, c.c_int]
        user.GetClassLongPtrW.restype = c.c_size_t
        handle = c.c_size_t()
        user.SendMessageTimeoutW(window_handle, 0x7F, 0 if small else 1, 0, 2, 1000, c.byref(handle))
        info.icon = handle.value or user.GetClassLongPtrW(window_handle, -34 if small else -14)
        if not info.icon:
            return {"logoVisible": False, "error": "Window has no small icon" if small else "Window has no taskbar large icon"}
    elif not shell.SHGetFileInfoW(str(path), 0, c.byref(info), c.sizeof(info), 0x100):
        raise RuntimeError("Windows could not extract shortcut icon")
    header = BitmapHeader(c.sizeof(BitmapHeader), 32, -32, 1, 32, 0, 4096, 0, 0, 0, 0)
    bits = c.c_void_p()
    dc = gdi.CreateCompatibleDC(None)
    bitmap = gdi.CreateDIBSection(dc, c.byref(header), 0, c.byref(bits), None, 0)
    if not dc or not bitmap or not bits.value:
        raise RuntimeError("Windows icon bitmap could not be created")
    old = gdi.SelectObject(dc, bitmap)
    try:
        c.memset(bits, 0, 4096)
        if not user.DrawIconEx(dc, 0, 0, info.icon, 32, 32, 0, None, 3):
            raise RuntimeError("Windows could not draw shortcut icon")
        pixels = c.string_at(bits, 4096)
        burgundy = sum(1 for b, g, r, a in zip(pixels[0::4], pixels[1::4], pixels[2::4], pixels[3::4])
                       if r > 40 and r > g + 20 and b > g * 1.2)
        return {"width": 32, "height": 32, "burgundyPixels": burgundy, "logoVisible": burgundy >= 10}
    finally:
        gdi.SelectObject(dc, old)
        gdi.DeleteObject(bitmap)
        gdi.DeleteDC(dc)
        if not window_handle:
            user.DestroyIcon(info.icon)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("shortcut", nargs="?", type=Path, default=desktop_path() / "Graham.lnk")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--small", action="store_true")
    parser.add_argument("--process-id", type=int)
    parser.add_argument("--wait-window-seconds", type=float, default=0)
    args = parser.parse_args()
    deadline = time.monotonic() + args.wait_window_seconds
    while True:
        try:
            handle = application_window(args.process_id) if args.process_id else None
            if not args.process_id:
                verify_shortcut_paths(args.shortcut)
            result = verify(args.shortcut, handle, args.small)
        except Exception as error:
            result = {"logoVisible": False, "error": str(error)}
        if result["logoVisible"] or time.monotonic() >= deadline:
            break
        time.sleep(0.1)
    if args.report:
        args.report.write_text(json.dumps(result), encoding="utf-8")
    print(json.dumps(result))
    if not result["logoVisible"]:
        raise SystemExit(1)
