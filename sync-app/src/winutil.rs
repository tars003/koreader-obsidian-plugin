//! Windows helpers for tray show/hide — bypasses egui when the window is hidden.

pub const WINDOW_TITLE: &str = "Kindle Vault Sync";

#[cfg(windows)]
pub fn find_hwnd() -> Option<isize> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    let title: Vec<u16> = WINDOW_TITLE
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: title is a valid NUL-terminated UTF-16 string.
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) };
    match hwnd {
        Ok(h) if !h.0.is_null() => Some(h.0 as isize),
        _ => None,
    }
}

/// Show + restore + focus the app window via Win32 (works from any thread).
#[cfg(windows)]
pub fn show_main_window() -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };

    let Some(raw) = find_hwnd() else {
        return false;
    };
    let hwnd = HWND(raw as *mut _);
    // SAFETY: hwnd from FindWindowW; ShowWindow/SetForegroundWindow are standard.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
    true
}

/// Hide the app window via Win32 (works from any thread).
#[cfg(windows)]
pub fn hide_main_window() -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    let Some(raw) = find_hwnd() else {
        return false;
    };
    let hwnd = HWND(raw as *mut _);
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    true
}

#[cfg(not(windows))]
pub fn show_main_window() -> bool {
    false
}

#[cfg(not(windows))]
pub fn hide_main_window() -> bool {
    false
}
