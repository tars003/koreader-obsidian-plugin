# Kindle Vault Sync

Windows desktop app that detects a plugged-in Kindle, runs md2kindle, copies the converted vault to the Kindle, and safely ejects.

## Requirements

- Windows 10/11
- Python on PATH with `md2kindle` installed (`pip install -e converter/`)
- A Kindle that mounts as a USB drive labeled `Kindle` (configurable)

## Build

Needs a Rust toolchain + a C linker (MSVC **or** MinGW).

```bash
# MinGW example (if MSVC Build Tools are not installed):
export PATH="/c/Users/YOU/tools/mingw64/bin:$PATH"
cargo +stable-x86_64-pc-windows-gnu build --release
```

Binary: `target/release/kindle-vault-sync.exe`

With MSVC Build Tools installed (normal path):

```bash
cargo build --release
```

## First run

1. Double-click `kindle-vault-sync.exe`
2. Set **Converter dir** to the folder that contains `md2kindle.toml`  
   (e.g. `C:/Users/Hp/Desktop/dev/software/labs/koreader-obsidian-plugin/converter`)
3. Click **Save Config**
4. Plug in the Kindle
5. Click **Sync Now** when the status says the Kindle was detected

The app minimizes to the system tray on window close. Right-click the tray icon for Show / Sync Now / Quit.

## What it does

1. Polls drive letters for volume label `Kindle`
2. On **Sync Now**:
   - `python -m md2kindle sync --config md2kindle.toml`
   - `robocopy <output_dir> <kindle>\<vault_folder> /MIR`
   - Safe-ejects the Kindle drive
3. Appends every step to `kindle-sync.log` next to the `.exe`

## Config

`config.toml` next to the `.exe`:

```toml
[paths]
converter_dir = "C:/path/to/converter"
python_path = ""   # empty = "python" on PATH

[kindle]
volume_label = "Kindle"
vault_folder = "obisidian-git-sync.kindle"

[behavior]
poll_interval_secs = 5
```
