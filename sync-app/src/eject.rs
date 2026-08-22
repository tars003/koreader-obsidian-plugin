//! Safe-eject a removable drive (Kindle). Tries DeviceIoControl first, then Shell.

/// Eject the drive at `root` (e.g. "D:\\" or "D:").
#[cfg(windows)]
pub fn eject_drive(root: &str) -> Result<(), String> {
    let letter = normalize_letter(root)?;

    // Give Explorer / AV a moment to release handles after robocopy.
    std::thread::sleep(std::time::Duration::from_millis(500));

    let mut errors = Vec::new();

    match eject_via_ioctl(&letter) {
        Ok(()) => {
            if wait_until_gone(&letter, 4000) {
                return Ok(());
            }
            errors.push("ioctl reported OK but drive still mounted".into());
        }
        Err(e) => errors.push(format!("ioctl: {e}")),
    }

    match eject_via_shell(&letter) {
        Ok(()) => {
            if wait_until_gone(&letter, 5000) {
                return Ok(());
            }
            errors.push("shell eject reported OK but drive still mounted".into());
        }
        Err(e) => errors.push(format!("shell: {e}")),
    }

    Err(errors.join("; "))
}

#[cfg(windows)]
fn normalize_letter(root: &str) -> Result<String, String> {
    let letter = root.trim().trim_end_matches(['\\', '/']).to_string();
    if letter.len() < 2 || !letter.as_bytes()[0].is_ascii_alphabetic() || !letter.contains(':') {
        return Err(format!("bad drive root: {root}"));
    }
    // "D:" form
    Ok(format!("{}:", letter.chars().next().unwrap().to_ascii_uppercase()))
}

/// True when the volume is no longer readable (ejected / gone).
#[cfg(windows)]
fn drive_gone(letter: &str) -> bool {
    use std::path::Path;
    let root = format!("{}\\", letter); // "D:\\"
    // exists() can still be true briefly; also probe volume info via detect path.
    if !Path::new(&root).exists() {
        return true;
    }
    // If GetVolumeInformation fails, treat as gone/unmounted.
    crate::detect::list_labeled_drives()
        .iter()
        .all(|d| !d.root.eq_ignore_ascii_case(&root))
}

#[cfg(windows)]
fn wait_until_gone(letter: &str, timeout_ms: u64) -> bool {
    let steps = (timeout_ms / 200).max(1);
    for _ in 0..steps {
        if drive_gone(letter) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    drive_gone(letter)
}

/// FSCTL_LOCK / DISMOUNT + IOCTL_STORAGE_EJECT_MEDIA
#[cfg(windows)]
fn eject_via_ioctl(letter: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    // \\.\D:
    let path = format!(r"\\.\{letter}");
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

    // SAFETY: wide is NUL-terminated path for CreateFileW.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|e| format!("CreateFile {path}: {e}"))?;

    if handle.is_invalid() {
        return Err(format!("CreateFile {path}: invalid handle"));
    }

    const FSCTL_LOCK_VOLUME: u32 = 0x0009_0018;
    const FSCTL_DISMOUNT_VOLUME: u32 = 0x0009_0020;
    const IOCTL_STORAGE_MEDIA_REMOVAL: u32 = 0x002D_4804;
    const IOCTL_STORAGE_EJECT_MEDIA: u32 = 0x002D_4808;

    let mut result = Ok(());

    // Lock (retry a few times — Explorer may hold briefly)
    let mut locked = false;
    for _ in 0..10 {
        let ok = unsafe {
            DeviceIoControl(handle, FSCTL_LOCK_VOLUME, None, 0, None, 0, None, None)
        };
        if ok.is_ok() {
            locked = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    if !locked {
        result = Err("could not lock volume (in use?)".into());
    }

    if result.is_ok() {
        let ok =
            unsafe { DeviceIoControl(handle, FSCTL_DISMOUNT_VOLUME, None, 0, None, 0, None, None) };
        if ok.is_err() {
            result = Err("dismount volume failed".into());
        }
    }

    if result.is_ok() {
        // PREVENT_MEDIA_REMOVAL = FALSE
        #[repr(C)]
        struct PreventMediaRemoval {
            prevent: u8,
        }
        let mut pmr = PreventMediaRemoval { prevent: 0 };
        let _ = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_MEDIA_REMOVAL,
                Some(&mut pmr as *mut _ as *mut _),
                std::mem::size_of::<PreventMediaRemoval>() as u32,
                None,
                0,
                None,
                None,
            )
        };

        let ok =
            unsafe { DeviceIoControl(handle, IOCTL_STORAGE_EJECT_MEDIA, None, 0, None, 0, None, None) };
        if ok.is_err() {
            result = Err("eject media ioctl failed".into());
        }
    }

    unsafe {
        let _ = CloseHandle(handle);
    }
    result
}

#[cfg(windows)]
fn eject_via_shell(letter: &str) -> Result<(), String> {
    use std::process::Command;

    // Shell.Application namespace 17 = "This PC"
    let script = format!(
        "$s = New-Object -ComObject Shell.Application; \
         $d = $s.Namespace(17).ParseName('{letter}'); \
         if ($null -eq $d) {{ throw 'drive not found in This PC' }}; \
         $d.InvokeVerb('Eject'); \
         Start-Sleep -Milliseconds 800"
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
