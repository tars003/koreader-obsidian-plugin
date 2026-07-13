# Obsidian → Kindle (via KOReader) — Phase 1 Notes

- **Date:** 2026-07-13
- **Status:** Design in progress — Phase 1 (scope, feasibility, architecture) settled; component details pending
- **Related:** `reference-prompt.md` (original request), `~/lab/obsidian-kindle/` (project root)

---

## Project goal (one line)

Read an Obsidian markdown vault on a jailbroken **Kindle Paperwhite 7th gen** running **KOReader**, with working `[[wikilink]]` navigation between notes, embedded remote images, and an Obsidian-style collapsible vault browser — **fully offline**.

## Environment (summary)

Full detail in `reference-prompt.md`. In short:

- **Source of truth:** private GitHub repo `tars003/obisidian-git-sync`.
- **Windows:** Obsidian desktop + `obsidian-git` (sync every 2 min / on open / close). Full local git clone.
- **Android:** Obsidian app + `obsidian-git` + GitSync app. Full local clone.
- **Kindle PW7:** jailbroken, KOReader installed. WiFi flaky; reads mostly **offline**.
- **Notes** contain `[[wikilinks]]` and remote HTTP image URLs (from the **Obsidian Clipper** Firefox extension). KOReader's default MD→HTML viewer can't fetch remote resources or follow cross-file links.

## Decisions made (Phase 1)

1. **Scope** — Build (a) a converter and (b) a KOReader plugin. **Sync is handled by existing tools**, not custom code.
2. **Offline-first** — Images are **pre-downloaded off-device** (on the Windows bridge); the Kindle receives a self-contained package. Chosen because the user reads mostly offline on old/slow hardware.
3. **Architecture = Approach A** — Bridge-side Python converter (`.md`→`.html`, embed images, rewrite links) → Syncthing → a small Lua KOReader plugin for cross-file link navigation.
   - *Approach C* (single EPUB, no plugin) was a strong, simpler alternative.
   - *Approach B* (pure on-device plugin) rejected — poor fit for offline + old hardware.
4. **Plugin scope expanded** — two modules: **link navigation** + a **collapsible Obsidian-style vault browser** (collapsible dirs, collapse/expand-all, focus-current-file).

## Key technical findings (verified via research)

- KOReader **already converts MD→HTML**: `frontend/apps/filemanager/filemanagerconverter.lua` + `frontend/apps/filemanager/lib/md.lua` (long-press a `.md` in the file manager → convert). This is the "default md viewer works fine."
- **Cross-file links do NOT work natively** — tapping a link to another local file yields *"Invalid or external link"* (issues #3461, #5941, #8611; relative paths are broken).
- **BUT** recent PRs **#11889** & **#12019** added `ReaderLink:registerScheme(...)` so a plugin can intercept relative/schemeless links → **this is the hook the navigation module uses.** It justifies a real, small on-device plugin.
- **Syncthing cannot pull from GitHub** (no git client). One node must bridge: keep a folder git-synced, share it via Syncthing. `d0nizam/kosyncthing_plus.koplugin` runs a Syncthing daemon **inside** KOReader on the Kindle.
- **Prior art for MD parsing in KOReader:** `omer-faruq/assistant.koplugin` (`assistant_mdparser.lua`, wraps hoedown / pure-Lua markdown).
- **No off-the-shelf collapsible-tree widget**, but buildable on `Menu`/`MenuItem` + `VerticalGroup` + `ButtonTable`. Prior art: `coverbrowser` `ListMenuItem`; FM long-press-Home → directory-tree shortcut.

## §1 — Architecture & data flow

```
  Obsidian vault (.md, remote images)          Windows bridge node
  ┌────────────────────────────┐  obsidian-git   ┌──────────────────────────┐
  │ Android  │  Windows │ GitHub│ ─────────────▶  │  /vault  (git working dir)│
  └────────────────────────────┘  keeps /vault    │           │              │
                                  in sync         │           ▼  (watcher)    │
                                                  │  ┌────────────────────┐  │
                                                  │  │ ① md2kindle         │  │  ← converter
                                                  │  │   .md → .html       │  │    (Python)
                                                  │  │   images→local      │  │
                                                  │  │   [[x]]→<a href>    │  │
                                                  │  └─────────┬──────────┘  │
                                                  │            ▼             │
                                                  │  /vault.kindle  (.html +  │
                                                  │   assets/, mirrors vault  │
                                                  │   folder tree, relative   │
                                                  │   links)                  │
                                                  └────────────┬─────────────┘
                                              Syncthing share (existing tool)
                                                                 │ WiFi
                                                                 ▼
                                          ┌────────────────────────────────────┐
  Kindle PW7                              │ ② obsidian.koplugin  (Lua)          │ ← the plugin
  KOReader                                │   ├ linkhandler                      │
                                          │   │  registerScheme → open target .html
                                          │   │  + back-stack                     │
                                          │   └ vaultbrowser                     │
                                          │      collapsible tree, focus current │
                                          │   renders via CRE (existing)         │
                                          └────────────────────────────────────┘
```

### Unit boundaries (each independently understandable & testable)

| Unit | Job | Knows about | Doesn't know about |
|---|---|---|---|
| **① `md2kindle`** (Python, on Windows) | Transform a `.md` vault into a self-contained `.html` vault: download remote images, rewrite `[[wikilinks]]` → links. **Preserves the vault folder tree.** | Markdown, Obsidian link syntax, HTML output | KOReader, Kindle, Lua |
| **② `obsidian.koplugin`** (Lua, on Kindle) | (a) Intercept cross-file link taps via `ReaderLink:registerScheme`, open target file, back-stack. (b) Collapsible vault browser. | KOReader APIs, local `.html` files | Obsidian, markdown, images |
| **Sync** (existing tools, config only) | Ship `/vault.kindle` → Kindle folder via Syncthing; keep `/vault` git-synced. | Syncthing + git | The other two units' internals |

**Why this split is healthy:** the converter is pure file-I/O, unit-testable on the dev box with sample vaults (no Kindle needed). The plugin is thin, pure-KOReader, with no dependency on how HTML was produced. The "contract" between them is just *relative `.html` links in a folder tree that mirrors the vault*. Sync is glue, not code.

## Plugin module: vaultbrowser (proposed behavior)

```
┌─ Vault ────────────────────────── [⤿collapse] [⤢expand] [◎focus] ─┐
│  ▾ Notes                                                           │
│    ▾ Projects                                                      │
│      ▸ KOReader plugin                                             │
│      ● koreader-design            ← current file (highlighted)     │
│    ▸ Reading                                                       │
│    ▾ Inbox                                                         │
│      · clipped-article-2026-07-10                                  │
│  ▸ Daily notes                                                     │
└────────────────────────────────────────────────────────────────────┘
   ▾ = expanded   ▸ = collapsed   ● file (open)   · file
```

- **Collapsible nested dirs** — `▸/▾` indicator; tap a folder row toggles children. Expand state remembered per session.
- **Collapse all / Expand all** — toolbar buttons (`⤿` / `⤢`).
- **Focus current file** — `◎focus` opens the browser with the tree auto-expanded along the path to the file being read, scrolled to + highlighted. Default when invoked while a file is open.

**Proposed defaults (to confirm):**
- Invoked from KOReader top menu (e.g., a "Vault" entry under Tools), full-screen.
- **Hide** non-content: `assets/`, `.obsidian/`, templates, and the `.html` extension.
- Show **note titles** (first `# H1`, else filename) instead of raw filenames.
- Sort: folders first, then alpha (configurable).
- **Read-only** — no rename/move/delete.

## Remaining design sections (to do)

- **§2** — Converter internals: trigger (file-watcher vs scheduled), incremental vs full rebuild, image dedup/caching, failure handling.
- **§3** — Wikilink resolution rules: `[[Note]]`, `[[Note|alias]]`, `[[Note#heading]]`, `![[embed]]`, duplicate basenames in subfolders, missing targets.
- **§4** — Kindle plugin internals: `registerScheme` mechanism, back-stack UX, vault-browser widget, menu/settings, broken-link handling.
- **§5** — Repo layout, language/library choices, dev & test workflow.
- **§6** — Edge cases & error handling; explicit "not doing now" list.

## Out of scope (for now)

- Highlighting / annotations / round-trip editing back to Obsidian.
- Live on-device image fetching (images are pre-embedded by the bridge).
- Custom sync/git code on the Kindle.
- File operations (rename/move/delete) in the vault browser.
