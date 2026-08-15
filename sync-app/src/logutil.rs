use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Thread-safe append-only logger. Also keeps an in-memory ring for the UI.
#[derive(Clone)]
pub struct Logger {
    path: Arc<std::path::PathBuf>,
    lines: Arc<Mutex<Vec<String>>>,
    max_ui_lines: usize,
}

impl Logger {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: Arc::new(path.as_ref().to_path_buf()),
            lines: Arc::new(Mutex::new(Vec::new())),
            max_ui_lines: 500,
        }
    }

    pub fn log(&self, msg: impl AsRef<str>) {
        let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
        let line = format!("[{ts}] {}", msg.as_ref());

        // File (best-effort)
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path.as_ref())
        {
            let _ = writeln!(f, "{line}");
        }

        // UI ring
        if let Ok(mut guard) = self.lines.lock() {
            guard.push(line);
            let excess = guard.len().saturating_sub(self.max_ui_lines);
            if excess > 0 {
                guard.drain(0..excess);
            }
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.lines.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn clear_ui(&self) {
        if let Ok(mut g) = self.lines.lock() {
            g.clear();
        }
    }
}
