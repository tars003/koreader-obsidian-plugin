//! Safe-eject a drive letter via Shell.Application COM (Windows).

/// Eject the drive at `root` (e.g. "E:\\").
/// Uses PowerShell + Shell.Application — no admin rights needed.
#[cfg(windows)]
pub fn eject_drive(root: &str) -> Result<(), String> {
    use std::process::Command;

    // Normalize to "E:" form that Shell.Application expects.
    let letter = root
        .trim()
        .trim_end_matches(['\\', '/'])
        .to_string();

    if letter.len() < 2 || !letter.as_bytes()[0].is_ascii_alphabetic() {
        return Err(format!("bad drive root: {root}"));
    }

    // Shell.Application namespace 17 = "My Computer"
    let script = format!(
        "$s = New-Object -ComObject Shell.Application; \
         $d = $s.Namespace(17).ParseName('{letter}'); \
         if ($null -eq $d) {{ throw 'drive not found' }}; \
         $d.InvokeVerb('Eject')"
    );

    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &script,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("spawn powershell: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = err.trim();
        if msg.is_empty() {
            return Err(format!(
                "eject failed (exit {})",
                out.status.code().unwrap_or(-1)
            ));
        }
        return Err(msg.to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn eject_drive(_root: &str) -> Result<(), String> {
    Err("eject is Windows-only".into())
}
