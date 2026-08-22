//! Windows user notifications (balloon tip — more reliable than WinRT toast without AUMID).

use crate::winutil;

/// Show a tray balloon notification. Best-effort; never panics.
/// Clicking the balloon / icon brings the Kindle Vault Sync window to the front.
pub fn show(title: &str, body: &str) {
    #[cfg(windows)]
    show_windows(title, body);
    #[cfg(not(windows))]
    {
        let _ = (title, body);
    }
}

#[cfg(windows)]
fn show_windows(title: &str, body: &str) {
    use std::process::Command;

    // Escape for single-quoted PowerShell strings.
    let title = title.replace('\'', "''");
    let body = body.replace('\'', "''");
    let win_title = winutil::WINDOW_TITLE.replace('\'', "''");

    // Use a temp .ps1 so quoting stays sane (click handler + Win32 show).
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class KVSWin {{
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
}}
"@
function Show-KVSWindow {{
  $h = [KVSWin]::FindWindow([NullString]::Value, '{win_title}')
  if ($h -eq [IntPtr]::Zero) {{ return }}
  [void][KVSWin]::ShowWindow($h, 9)  # SW_RESTORE
  [void][KVSWin]::SetForegroundWindow($h)
}}
$n = New-Object System.Windows.Forms.NotifyIcon
$n.Icon = [System.Drawing.SystemIcons]::Information
$n.Text = 'Kindle Vault Sync'
$n.Visible = $true
$n.BalloonTipTitle = '{title}'
$n.BalloonTipText = '{body}'
$n.BalloonTipIcon = [System.Windows.Forms.ToolTipIcon]::Info
$n.add_BalloonTipClicked({{ Show-KVSWindow }})
$n.add_Click({{ Show-KVSWindow }})
$n.ShowBalloonTip(10000)
Start-Sleep -Seconds 12
$n.Visible = $false
$n.Dispose()
"#
    );

    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "kvs-notify-{}.ps1",
        std::process::id()
    ));
    if std::fs::write(&path, script).is_err() {
        return;
    }

    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        &path.to_string_lossy(),
    ])
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.spawn();
}
