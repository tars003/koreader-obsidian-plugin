# obsidian.koplugin

KOReader plugin for reading an Obsidian `.html` vault (produced by `md2kindle`) on a Kindle / Kobo / Android e-ink device.

## Features

- **Wikilink navigation** — tap a `[[Note]]`-style link to open the referenced `.html` note. Back via `Obsidian Vault → Go back to previous note`.
- **Vault browser** — full-screen collapsible folder tree with:
  - Expand / collapse any folder
  - Collapse all / Expand all
  - Focus current file (auto-expand path to the file you're reading)
  - State persisted across sessions
- **Auto-prompt** — asks for the vault root path on first run.

## Requirements

- KOReader with a recent ReaderLink (supports `registerScheme("")` — PRs #11889 / #12019, available in KOReader 2023.06+).
- The vault root populated by the `md2kindle` converter (see `../converter/`), containing `.html` files with relative wikilinks and e-ink-optimized images. Synced to the device via Syncthing or USB.

## Install

Copy `obsidian.koplugin/` into `<device>/koreader/plugins/obsidian.koplugin/`, then restart KOReader.

## First run

1. Restart KOReader after installing.
2. You'll be prompted for the vault root path (e.g. `/mnt/us/vault.kindle` on Kindle).
3. Open any `.html` file from the KOReader file manager. Tap a wikilink to navigate.

## Manual deploy

```bash
# from the repo root
scp -r plugin/obsidian.koplugin/ root@kindle:/mnt/us/koreader/plugins/
# or via USB / Syncthing folder
```

## Design

- Spec: `../docs/design.md` (§7)
- Plan: `../docs/plans/2026-07-13-obsidian-koplugin.md`
