//! Windows user notifications (balloon tip — more reliable than WinRT toast without AUMID).

/// Show a tray balloon notification. Best-effort; never panics.
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

    // System.Windows.Forms balloon works without a registered AppUserModelID.
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         Add-Type -AssemblyName System.Drawing; \
         $n = New-Object System.Windows.Forms.NotifyIcon; \
         $n.Icon = [System.Drawing.SystemIcons]::Information; \
         $n.Visible = $true; \
         $n.BalloonTipTitle = '{title}'; \
         $n.BalloonTipText = '{body}'; \
         $n.BalloonTipIcon = [System.Windows.Forms.ToolTipIcon]::Info; \
         $n.ShowBalloonTip(8000); \
         Start-Sleep -Seconds 9; \
         $n.Dispose()"
    );

    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &script,
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
