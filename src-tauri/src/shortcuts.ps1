$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;
public static class GrahamShortcutShell {
    [DllImport("shell32.dll", CharSet=CharSet.Unicode)]
    public static extern void SHChangeNotify(int change, uint flags, string item, IntPtr unused);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)]
    private static extern uint GetFinalPathNameByHandle(SafeFileHandle file, StringBuilder path, uint size, uint flags);
    public static string FinalPath(string path) {
        try {
            using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete)) {
                var result = new StringBuilder(32768);
                uint length = GetFinalPathNameByHandle(file.SafeFileHandle, result, (uint)result.Capacity, 0);
                return length > 0 && length < result.Capacity ? result.ToString() : null;
            }
        } catch (IOException) { return null; }
          catch (UnauthorizedAccessException) { return null; }
          catch (ArgumentException) { return null; }
    }
}
'@
$desktop = [Environment]::GetFolderPath('DesktopDirectory', [Environment+SpecialFolderOption]::DoNotVerify)
$programs = [Environment]::GetFolderPath('Programs', [Environment+SpecialFolderOption]::DoNotVerify)
if (!$desktop -or !$programs) { throw 'Desktop or Programs folder is unavailable' }
[void][IO.Directory]::CreateDirectory($desktop)
[void][IO.Directory]::CreateDirectory($programs)
$links = @((Join-Path $desktop 'Graham.lnk'), (Join-Path $programs 'Graham.lnk'))
$shell = New-Object -ComObject WScript.Shell
$pinned = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'Microsoft/Internet Explorer/Quick Launch/User Pinned/TaskBar'
if (Test-Path -LiteralPath $pinned) {
    $ownedTarget = [GrahamShortcutShell]::FinalPath($env:GRAHAM_SHORTCUT_TARGET)
    if (!$ownedTarget) { throw 'Cannot verify the installed executable path' }
    foreach ($file in Get-ChildItem -LiteralPath $pinned -Filter '*.lnk' -File) {
        try {
            $existing = $shell.CreateShortcut($file.FullName)
            if ([IO.Path]::GetFileName($existing.TargetPath) -ine 'graham.exe') { continue }
            $targetRoot = [IO.Path]::GetPathRoot($existing.TargetPath)
            $ownedRoot = [IO.Path]::GetPathRoot($env:GRAHAM_SHORTCUT_TARGET)
            if ($targetRoot.StartsWith('\\') -and $targetRoot -ine $ownedRoot) { continue }
            $resolved = [GrahamShortcutShell]::FinalPath($existing.TargetPath)
            if ($resolved -and [String]::Equals($resolved, $ownedTarget, [StringComparison]::OrdinalIgnoreCase)) { $links += $file.FullName }
        } catch { continue }
    }
}
foreach ($path in $links) {
    $existed = Test-Path -LiteralPath $path
    $link = $shell.CreateShortcut($path)
    $link.TargetPath = $env:GRAHAM_SHORTCUT_TARGET
    $link.IconLocation = $env:GRAHAM_SHORTCUT_ICON + ',0'
    $link.WorkingDirectory = $env:GRAHAM_SHORTCUT_DIRECTORY
    $link.Arguments = ''
    $link.Description = 'Graham'
    $link.Save()
    $change = if ($existed) { 0x2000 } else { 0x2 }
    [GrahamShortcutShell]::SHChangeNotify($change, 0x1005, $path, [IntPtr]::Zero)
}
