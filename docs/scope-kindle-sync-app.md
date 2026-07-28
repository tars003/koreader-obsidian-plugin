# Kindle Vault Sync — Scope Document

**Status:** Approved for implementation
**Date:** 2026-07-18
**Repo:** `obsidian-kindle` (this repo)
**Companion docs:** `docs/design.md` (converter + plugin architecture), `docs/glossary.md` (locked vocabulary)

---

## 1. One-liner

A lightweight Rust desktop app for Windows that detects when a Kindle is plugged in via USB, runs the existing md2kindle converter, and copies the converted vault to the Kindle — with a confirm popup so the user stays in control.

## 2. Problem it solves

Today, getting the converted vault onto the Kindle is manual:

1. Plug Kindle into PC
2. Wait for it to mount
3. Manually run the converter (or wait for the 5-min Task Scheduler job)
4. Manually copy the output folder to the Kindle's USB drive
5. Safely eject

Steps 3-4 are the pain. The converter already runs on a schedule, but the *copy to Kindle* only happens when the user remembers to do it. This app automates the "detect → confirm → convert → copy" flow so the user just plugs in and clicks one button.

## 3. Features (v1)

- **Kindle detection:** polls Windows drive letters every ~5 seconds, matches by volume label (configurable, default "Kindle"). No admin rights needed.
- **Confirm before sync:** when Kindle is detected, the app highlights a "Sync Now" button (or shows a popup). The user clicks to start. Nothing happens automatically without user consent.
- **Runs the existing converter:** calls `python -m md2kindle sync --config <path>` in the converter directory. Does NOT reimplement any conversion logic.
- **Copies output to Kindle:** mirrors the converter's `output_dir` (read from `md2kindle.toml`) to the Kindle's vault folder. Uses `robocopy /MIR` on Windows.
- **Progress/status display:** shows what's happening (detecting, converting, copying, done) in the UI.
- **Config persistence:** saves settings (converter path, Kindle label, vault folder name) to a local config file so the user only sets them once.
- **First-run setup:** on first launch, asks the user where the converter code directory is (the folder containing `md2kindle.toml`). Derives everything else from that path.
- **Runs in background (system tray):** the app minimizes to the system tray and watches for the Kindle continuously. When the Kindle is detected, it shows a Windows notification / balloon popup asking "Sync vault now?". The user doesn't need to keep a window open.
- **Auto-eject after sync:** once the copy completes successfully, the app safely ejects the Kindle drive so the user can just unplug the cable without worrying about data corruption.
- **Operation logging:** all main operations are written to a timestamped log file (e.g., `kindle-sync.log` next to the config). Each entry is `[YYYY-MM-DD HH:MM:SS] <message>`. Examples: "Kindle detected at E:\", "Converter started", "Converter finished (623 files)", "Copy complete", "Kindle ejected", "Error: converter failed". The log file is append-only, plain text, human-readable.

## 4. Non-goals (v1)

- **Not a converter.** The conversion logic lives in `converter/md2kindle/` (Python). This app just calls it.
- **Not cross-platform (yet).** Windows-only for v1. The detection mechanism (`GetVolumeInformationW`) and copy tool (`robocopy`) are Windows-specific. Linux/macOS support is a future item.
- **Not a sync daemon.** It doesn't run the converter on a schedule. The existing Task Scheduler job handles that. This app only acts when the Kindle is plugged in AND the user confirms.
- **Not a file manager.** No browsing, editing, or deleting files on the Kindle from within the app.

## 5. Tech stack

- **Language:** Rust (stable toolchain)
- **GUI:** egui via eframe (immediate-mode, pure Rust, very basic UI is its sweet spot)
- **Windows API:** `windows` crate (or `winapi`) for `GetVolumeInformationW` drive detection
- **Process spawning:** `std::process::Command` to call Python and robocopy
- **Config format:** TOML (matches the existing `md2kindle.toml` convention)
- **Config location:** next to the `.exe`, or `%APPDATA%/kindle-vault-sync/config.toml`
- **System tray:** `tray-icon` crate (or equivalent) for minimize-to-tray + balloon notifications
- **Safe eject:** Windows `CM_Request_Device_Eject` API (via `windows` crate) or `devcon eject` command
- **Logging:** simple append-to-file, no external crate needed (`std::fs::OpenOptions`)

## 6. Architecture

```
┌─ Kindle Vault Sync (Rust binary) ─────────────────────────┐
│                                                           │
│  ┌─ UI thread (egui) ─┐    ┌─ Background thread ──────┐  │
│  │                     │    │                           │  │
│  │  Config fields      │    │  Poll drives every 5s     │  │
│  │  Sync button        │◄───┤  Match volume label       │  │
│  │  Status line        │    │  Set "kindle_found" flag  │  │
│  │  Log area           │    │                           │  │
│  └─────────────────────┘    └───────────────────────────┘  │
│                                                           │
│  ┌─ Sync worker (spawned on button click) ──────────────┐ │
│  │                                                       │ │
│  │  1. cd to converter dir                               │ │
│  │  2. Run: python -m md2kindle sync --config <toml>     │ │
│  │  3. Read output_dir from toml                         │ │
│  │  4. robocopy <output_dir> <kindle_drive>\<vault> /MIR │ │
│  │  5. Report success/failure back to UI                 │ │
│  └───────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────┘
```

**Key principle:** this app is a *thin orchestrator*. It doesn't own any conversion logic. It calls the existing `md2kindle` Python package and the existing `robocopy` Windows tool. If the converter changes, this app doesn't need to change.

## 7. UI description

```
┌─ Kindle Vault Sync ──────────────────────────┐
│                                              │
│  Converter dir: [C:/Users/.../converter  ]   │
│  Kindle label:  [Kindle                  ]   │
│  Vault folder:  [obisidian-git-sync.kindle]  │
│                                              │
│  Status: Waiting for Kindle...               │
│                                              │
│         [ Sync Now ]    [ Save Config ]      │
│                                              │
│  ─── Log ─────────────────────────────────── │
│  [12:03] Kindle detected at E:\              │
│  [12:03] Running converter...                │
│  [12:03] [287/623] Clippings/some-note.md    │
│  [12:04] Converter done. Copying to Kindle...│
│  [12:04] Done. Safe to unplug.               │
└──────────────────────────────────────────────┘
```

- **Converter dir:** path to the folder containing `md2kindle.toml`. Set on first run, saved to config.
- **Kindle label:** the volume label to match. Default "Kindle".
- **Vault folder:** the folder name on the Kindle where the vault lives. Default "obisidian-git-sync.kindle".
- **Status:** one line showing current state (waiting / detected / converting / copying / done / error).
- **Sync Now:** enabled only when Kindle is detected. Triggers the sync worker.
- **Save Config:** writes the three fields to the config file.
- **Log:** scrollable area showing converter stdout + app messages. Gives the user visibility into what's happening.

## 8. Config

Stored as TOML (e.g., `config.toml` next to the `.exe`):

```toml
[paths]
converter_dir = "C:/Users/tars/lab/obsidian-kindle/converter"

[kindle]
volume_label = "Kindle"
vault_folder = "obisidian-git-sync.kindle"

[behavior]
poll_interval_secs = 5
```

- On first launch (no config file exists): the app shows the UI with empty fields and prompts the user to set the converter directory. Everything else has sensible defaults.
- The converter's own config (`md2kindle.toml`) is read by the converter, not by this app. This app only needs to know *where* it is (to pass `--config` and to `cd` into the right directory).
- The `output_dir` for robocopy is read from `md2kindle.toml` at sync time (so if the user changes it, this app picks it up automatically).

## 9. Sync flow (step by step)

1. App starts → loads config → starts background detection thread
2. Background thread polls drives every `poll_interval_secs`
3. User plugs in Kindle → drive appears with label "Kindle"
4. Background thread detects it → sets flag → UI updates: "Kindle detected at E:\"
5. "Sync Now" button becomes enabled
6. User clicks "Sync Now"
7. App spawns sync worker thread:
   a. `cd` to `converter_dir`
   b. Run `python -m md2kindle sync --config md2kindle.toml`
   c. Stream converter stdout to the log area (so user sees progress)
   d. When converter exits: read `output_dir` from `md2kindle.toml`
   e. Run `robocopy <output_dir> <kindle_drive>\<vault_folder> /MIR /R:1 /W:1`
   f. Check robocopy exit code (< 8 = success)
8. UI updates: "Done. Ejecting Kindle..."
9. App safely ejects the Kindle drive (Windows `CM_Request_Device_Eject` or equivalent)
10. UI updates: "Kindle ejected. You can unplug now."
11. All steps (detect, convert start, convert end, copy start, copy end, eject, errors) are appended to the log file with timestamps.
12. App returns to background (system tray), resumes polling.

## 10. Error handling

- **Kindle unplugged mid-converter:** converter finishes (it doesn't need the Kindle). Copy step detects the drive is gone → shows "Kindle disconnected during sync. Plug it back in and try again."
- **Kindle unplugged mid-copy:** robocopy fails → shows error. Next sync will fix any partial state (robocopy /MIR is idempotent).
- **Converter fails (non-zero exit):** shows the last ~500 chars of converter stderr in an error dialog. Does NOT attempt the copy.
- **Python not found:** checks `python --version` before running. If not found, shows "Python not found. Is it installed and on PATH?"
- **Config file missing/corrupt:** falls back to defaults, shows the config UI for the user to re-enter paths.
- **Drive label matches but it's not actually the Kindle:** user sees the wrong drive in the status line, can cancel. (Future: also check for a marker file like `.kindle_vault` in the drive root.)

## 11. Build & deploy

- **Build:** `cargo build --release` → produces a single `.exe` in `target/release/`
- **Binary size:** ~5-10MB (statically linked, no runtime dependencies beyond Windows DLLs)
- **Distribution:** copy the `.exe` to the user's machine. No installer needed. Config file is created on first run.
- **Python dependency:** the user's existing Python + md2kindle venv must be on PATH (or the app could be configured with the full path to `python.exe` — future item).

## 12. Future / out of scope for v1

- **Auto-sync without confirm:** optional "auto-sync on plug-in" toggle for users who don't want the confirm step.
- **Linux support:** use `udev`/`udisks2` for detection, `rsync` for copy.
- **Progress bar:** parse converter output for `[N/M]` lines and show a progress bar instead of raw log text.
- **Multiple Kindles:** support multiple devices with different labels/vault folders.
- **Marker file check:** verify the detected drive is actually the Kindle by checking for a known file (e.g., `system/version.txt` which all Kindles have).
- **Log rotation:** cap the log file size or rotate daily. v1 just appends forever (it's tiny).

---

## Relationship to existing code

This app does NOT replace or modify any existing code in this repo:

- `converter/md2kindle/` — unchanged. Called as a subprocess.
- `plugin/obsidian.koplugin/` — unchanged. Runs on the Kindle, unrelated to this app.
- `docs/design.md` — the converter + plugin architecture. This app is a *deployment convenience* on top of that architecture.

The app lives in a new directory: `sync-app/` (or `tools/kindle-sync/`) at the repo root.
