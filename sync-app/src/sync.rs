//! Sync worker: run md2kindle, then robocopy output to the Kindle.

use crate::config::AppConfig;
use crate::detect;
use crate::logutil::Logger;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncPhase {
    Idle,
    Converting,
    Copying,
    Ejecting,
    Done,
    Error,
}

#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub phase: SyncPhase,
    pub detail: String,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            phase: SyncPhase::Idle,
            detail: String::new(),
        }
    }
}

/// Shared status the UI polls.
pub type SharedStatus = Arc<std::sync::Mutex<SyncStatus>>;

pub fn new_shared_status() -> SharedStatus {
    Arc::new(std::sync::Mutex::new(SyncStatus::default()))
}

fn set_status(status: &SharedStatus, phase: SyncPhase, detail: impl Into<String>) {
    if let Ok(mut g) = status.lock() {
        g.phase = phase;
        g.detail = detail.into();
    }
}

/// Spawn the full sync on a background thread. Returns immediately.
/// `busy` is set true for the duration and cleared when finished.
pub fn spawn_sync(
    cfg: AppConfig,
    kindle_root: String,
    logger: Logger,
    status: SharedStatus,
    busy: Arc<AtomicBool>,
) {
    if busy.swap(true, Ordering::SeqCst) {
        logger.log("Sync already running — ignored.");
        return;
    }

    thread::spawn(move || {
        let result = run_sync(&cfg, &kindle_root, &logger, &status);
        match result {
            Ok(msg) => {
                set_status(&status, SyncPhase::Done, msg.clone());
                logger.log("Sync complete.");
                crate::notify::show("Kindle Vault Sync", &msg);
            }
            Err(e) => {
                set_status(&status, SyncPhase::Error, e.clone());
                logger.log(format!("Error: {e}"));
                crate::notify::show("Kindle Vault Sync — Error", &e);
            }
        }
        busy.store(false, Ordering::SeqCst);
    });
}

fn run_sync(
    cfg: &AppConfig,
    kindle_root: &str,
    logger: &Logger,
    status: &SharedStatus,
) -> Result<String, String> {
    if !cfg.is_configured() {
        return Err("Converter dir not set (or md2kindle.toml missing).".into());
    }

    // 1. Python check
    let python = cfg.python_cmd();
    logger.log(format!("Checking {python} ..."));
    let ver = Command::new(&python)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|_| {
            format!("Python not found (`{python}`). Is it installed and on PATH?")
        })?;
    if !ver.status.success() {
        return Err(format!("Python not found (`{python}`). Is it installed and on PATH?"));
    }
    let ver_text = String::from_utf8_lossy(&ver.stdout);
    let ver_err = String::from_utf8_lossy(&ver.stderr);
    let shown = if !ver_text.trim().is_empty() {
        ver_text.trim()
    } else {
        ver_err.trim()
    };
    logger.log(format!("Python OK: {shown}"));

    // 2. Run converter
    set_status(status, SyncPhase::Converting, "Running converter...");
    logger.log("Converter started");

    let converter_dir = PathBuf::from(&cfg.paths.converter_dir);
    let toml_path = cfg.converter_toml();

    // Force UTF-8 stdio so progress prints with non-ASCII never crash on cp1252.
    let mut child = Command::new(&python)
        .current_dir(&converter_dir)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .args([
            "-m",
            "md2kindle",
            "sync",
            "--config",
            toml_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("md2kindle.toml"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn converter: {e}"))?;

    // Stream stdout
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            let trimmed = line.trim_end().to_string();
            if !trimmed.is_empty() {
                logger.log(&trimmed);
                set_status(status, SyncPhase::Converting, trimmed);
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait converter: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        // Keep the last ~12 lines of stderr — more useful than a char-reversed slice.
        let tail: String = err
            .lines()
            .rev()
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        for line in tail.lines() {
            if !line.trim().is_empty() {
                logger.log(line);
            }
        }
        let msg = if tail.trim().is_empty() {
            format!(
                "Converter failed (exit {}).",
                output.status.code().unwrap_or(-1)
            )
        } else {
            format!("Converter failed:\n{tail}")
        };
        return Err(msg);
    }
    logger.log("Converter finished");

    // 3. Confirm Kindle still present
    let drive_still = detect::find_kindle(&cfg.kindle.volume_label);
    let root = match drive_still {
        Some(d) => d.root,
        None => {
            // Fall back to the root we were given, but warn if gone
            if !PathBuf::from(kindle_root).exists() {
                return Err(
                    "Kindle disconnected during sync. Plug it back in and try again.".into(),
                );
            }
            kindle_root.to_string()
        }
    };

    // 4. Read output_dir and robocopy
    let output_dir = cfg.read_output_dir()?;
    if !output_dir.is_dir() {
        return Err(format!(
            "Converter output_dir does not exist: {}",
            output_dir.display()
        ));
    }

    let dest = PathBuf::from(&root).join(&cfg.kindle.vault_folder);
    set_status(
        status,
        SyncPhase::Copying,
        format!("Copying to {} ...", dest.display()),
    );
    logger.log(format!(
        "Copy started: {} → {}",
        output_dir.display(),
        dest.display()
    ));

    // robocopy exit codes: 0-7 = success-ish, >= 8 = failure
    let rc = Command::new("robocopy")
        .args([
            output_dir.to_string_lossy().as_ref(),
            dest.to_string_lossy().as_ref(),
            "/MIR",
            "/R:1",
            "/W:1",
            "/NFL", // no file list (keep log quieter)
            "/NDL",
            "/NP",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| format!("spawn robocopy: {e}"))?;

    let code = rc.code().unwrap_or(16);
    if code >= 8 {
        // Drive may have vanished mid-copy
        if !PathBuf::from(&root).exists() {
            return Err("Kindle disconnected during copy. Plug it back in and try again.".into());
        }
        return Err(format!("robocopy failed (exit {code})"));
    }
    logger.log(format!("Copy complete (robocopy exit {code})"));

    // 5. Safe eject
    set_status(status, SyncPhase::Ejecting, "Ejecting Kindle...");
    logger.log("Ejecting Kindle...");
    let done_msg = match crate::eject::eject_drive(&root) {
        Ok(()) => {
            logger.log("Kindle ejected");
            "Sync complete. Kindle ejected — safe to unplug.".to_string()
        }
        Err(e) => {
            // Non-fatal: copy already succeeded
            logger.log(format!("Eject warning: {e}. Copy OK — eject manually in Explorer."));
            format!(
                "Sync complete (copy OK). Eject failed — use Safely Remove in Explorer. ({e})"
            )
        }
    };

    Ok(done_msg)
}
