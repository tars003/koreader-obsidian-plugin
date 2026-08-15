//! Kindle drive detection via Windows volume labels.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedDrive {
    /// e.g. "E:\\"
    pub root: String,
    /// e.g. "Kindle"
    pub label: String,
}

/// List fixed/removable drive roots that currently have a volume label.
#[cfg(windows)]
pub fn list_labeled_drives() -> Vec<DetectedDrive> {
    use windows::core::PWSTR;
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;

    let mut out = Vec::new();

    // A: through Z:
    for letter in b'A'..=b'Z' {
        let root = format!("{}:\\", letter as char);
        let root_wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();

        let mut label_buf = [0u16; 256];
        let mut fs_flags = 0u32;
        let mut max_comp = 0u32;
        let mut serial = 0u32;
        let mut fs_name = [0u16; 64];

        // SAFETY: root_wide is a valid NUL-terminated UTF-16 path; buffers are stack arrays.
        let ok = unsafe {
            GetVolumeInformationW(
                PWSTR(root_wide.as_ptr() as *mut u16),
                Some(&mut label_buf),
                Some(&mut serial),
                Some(&mut max_comp),
                Some(&mut fs_flags),
                Some(&mut fs_name),
            )
        };

        if ok.is_err() {
            continue;
        }

        let end = label_buf.iter().position(|&c| c == 0).unwrap_or(label_buf.len());
        let label = String::from_utf16_lossy(&label_buf[..end]);
        if label.is_empty() {
            continue;
        }

        out.push(DetectedDrive { root, label });
    }

    out
}

#[cfg(not(windows))]
pub fn list_labeled_drives() -> Vec<DetectedDrive> {
    Vec::new()
}

/// Find the first drive whose volume label matches `wanted` (case-insensitive).
pub fn find_kindle(wanted_label: &str) -> Option<DetectedDrive> {
    let wanted = wanted_label.trim();
    if wanted.is_empty() {
        return None;
    }
    list_labeled_drives()
        .into_iter()
        .find(|d| d.label.eq_ignore_ascii_case(wanted))
}
