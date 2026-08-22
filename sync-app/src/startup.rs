//! Enable/disable "run at Windows logon" via HKCU Run key.

const VALUE_NAME: &str = "KindleVaultSync";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Register or unregister the current exe to start at user logon.
pub fn set_run_at_startup(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe_str = exe.to_string_lossy().to_string();

    if enabled {
        // Quote path so spaces are safe.
        let data = format!("\"{exe_str}\"");
        let status = run_reg(&[
            "add",
            RUN_KEY,
            "/v",
            VALUE_NAME,
            "/t",
            "REG_SZ",
            "/d",
            &data,
            "/f",
        ])?;
        if !status {
            return Err("reg add failed".into());
        }
    } else {
        // Ignore failure if value was already absent.
        let _ = run_reg(&["delete", RUN_KEY, "/v", VALUE_NAME, "/f"]);
    }
    Ok(())
}

/// Whether our Run key is currently present.
pub fn is_run_at_startup() -> bool {
    run_reg(&["query", RUN_KEY, "/v", VALUE_NAME]).unwrap_or(false)
}

fn run_reg(args: &[&str]) -> Result<bool, String> {
    use std::process::Command;

    let mut cmd = Command::new("reg");
    cmd.args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let status = cmd.status().map_err(|e| format!("reg: {e}"))?;
    Ok(status.success())
}
