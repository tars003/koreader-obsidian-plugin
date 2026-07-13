# Design Spec — Obsidian → Kindle (via KOReader)

- **Status:** Approved (design phase) — ready for implementation planning
- **Date:** 2026-07-13
- **Project root:** `~/lab/obsidian-kindle/`
- **Companions:** `phase-1-notes.md` (design log), `reference-prompt.md` (original request)

---

## 1. Overview

Read an Obsidian markdown vault on a jailbroken **Kindle Paperwhite 7th gen** running **KOReader**, fully offline, with:

1. Working **`[[wikilink]]` navigation** between notes (KOReader cannot follow cross-file links natively).
2. **Embedded remote images** (notes from the Obsidian Clipper Firefox extension store images as HTTP URLs, which KOReader cannot fetch).
3. An **Obsidian-style collapsible vault browser** (collapsible dirs, collapse/expand-all, focus-current-file).

## 2. Background & environment

- **Source of truth:** private GitHub repo `tars003/obisidian-git-sync`.
- **Windows:** Obsidian desktop + `obsidian-git` (sync every 2 min / on open / close). Full local git clone at `/vault`.
- **Android:** Obsidian app + `obsidian-git` + GitSync app. Full local clone. Sync between Android and Windows is already flawless.
- **Kindle PW7:** jailbroken, KOReader installed. WiFi flaky; reads **mostly offline**. Touchscreen-only (no physical page-turn buttons).
- **Content:** notes contain `[[wikilinks]]` and remote HTTP image URLs. KOReader's built-in MD→HTML viewer (`filemanagerconverter.lua` + `lib/md.lua`) renders text fine but cannot fetch remote resources or follow links to other local files.

## 3. Requirements & constraints

- **Offline-first:** all network work (image download) happens **off the Kindle**, on the Windows bridge. The Kindle receives a self-contained package.
- **Read-only on Kindle:** no editing, no highlighting, no round-trip back to Obsidian (Phase 1).
- **Old/slow target hardware:** heavy processing belongs off-device; the on-device plugin must be lightweight.
- **Sync via existing tools** (Syncthing + git) — no custom sync code.
- **Robust to broken links/missing images:** clipped web content references many things not in the vault; these must degrade gracefully and never break reading.

## 4. Non-goals (out of scope, Phase 1)

Highlighting/annotations; editing round-trip to Obsidian; live on-device image fetching; custom git/sync on the Kindle; transclusion inlining; file operations (rename/move/delete) in the browser; math (`$$…$$`) and syntax-highlight rendering.

## 5. Architecture

**Approach A** — a Python converter on the Windows bridge produces a self-contained HTML vault; Syncthing ships it to the Kindle; a small Lua KOReader plugin provides cross-file link navigation and a vault browser.

```
  Obsidian vault (.md, remote images)          Windows bridge node
  ┌────────────────────────────┐  obsidian-git   ┌──────────────────────────┐
  │ Android  │  Windows │ GitHub│ ─────────────▶  │  /vault  (git working dir)│
  └────────────────────────────┘  keeps /vault    │           │              │
                                  in sync         │           ▼  Task Sched   │
                                                  │  ┌────────────────────┐  │
                                                  │  │ ① md2kindle         │  │  converter (Python)
                                                  │  │   .md → .html       │  │  every ~5 min
                                                  │  │   images→local+opt  │  │
                                                  │  │   [[x]]→<a href>    │  │
                                                  │  └─────────┬──────────┘  │
                                                  │            ▼             │
                                                  │  /vault.kindle (.html +  │
                                                  │   assets/, mirrors vault │
                                                  │   tree, relative links)  │
                                                  └────────────┬─────────────┘
                                              Syncthing share (existing tool)
                                                                 │ WiFi
                                                                 ▼
                                          ┌────────────────────────────────────┐
  Kindle PW7                              │ ② obsidian.koplugin  (Lua)          │ ← plugin
  KOReader                                │   ├ linkhandler                      │
                                          │   │  registerScheme("") → open .html │
                                          │   │  + back-stack (menu Back)        │
                                          │   └ vaultbrowser                     │
                                          │      collapsible tree, focus current │
                                          │   renders via CREngine (built-in)    │
                                          └────────────────────────────────────┘
```

### Unit boundaries

| Unit | Job | Knows about | Independent of |
|---|---|---|---|
| **① `md2kindle`** (Python, Windows) | Transform `.md` vault → self-contained `.html` vault: download/optimize images, rewrite `[[wikilinks]]` → links. Preserves folder tree. | Markdown, Obsidian link syntax, HTML | KOReader, Kindle, Lua |
| **② `obsidian.koplugin`** (Lua, Kindle) | (a) Intercept cross-file link taps via `registerScheme("")`, open target, back-stack. (b) Collapsible vault browser. | KOReader APIs, local `.html` files | Obsidian, markdown, images |
| **Sync** (config only) | Ship `/vault.kindle` → Kindle folder via Syncthing; keep `/vault` git-synced. | Syncthing + git | internals of the other two |

**Contract between units:** *relative `.html` links inside a folder tree that mirrors the vault.* Nothing else couples them — the converter is unit-testable with fixture vaults (no Kindle), and the plugin is pure KOReader (no knowledge of how HTML was produced).

> **Sync topology note:** Syncthing has no git client, so one node must bridge. The Windows box already keeps `/vault` git-synced (obsidian-git); running the Syncthing desktop app sharing that same folder ships it to the Kindle, which runs `kosyncthing_plus.koplugin` (a Syncthing daemon inside KOReader).

## 6. Unit ① — `md2kindle` converter (Python)

### 6.1 Modules

```
md2kindle/
├── cli.py        # `python -m md2kindle sync` — one-shot incremental pass
├── pipeline.py   # scan → resolve → render → images → write
├── manifest.py   # state: per-source hash + per-image {url → cached file}
├── resolver.py   # [[wikilink]] → target path  (see §8)
├── images.py     # download / dedup / downscale+grayscale / format-normalize
├── renderer.py   # markdown → HTML (+ e-ink CSS injection)
├── layout.py     # mirror folder tree; shared assets/; orphan cleanup
└── config.py     # loads md2kindle.toml
```

Each module has one job and a pure interface (e.g., `images.py` takes `{urls} + cache dir`, returns `{url → relative_path}`).

### 6.2 Trigger — scheduled incremental

`python -m md2kindle sync` via **Windows Task Scheduler every ~5 min** (configurable). Chosen over a file-watcher because obsidian-git rewrites many files at once and Windows watchers drop events under that churn. A few minutes of lag is irrelevant for this use case. A debounced watcher mode is available as an option for near-real-time.

### 6.3 Incremental processing + manifest

`assets/.md2kindle/manifest.json` records, per source `{path, content_hash, mtime, output, images_used}` and per image `{url → {content_hash, local_path, etag}}`. A pass:

1. Re-convert only `.md` whose hash changed.
2. Remove outputs whose source disappeared (orphan cleanup).
3. Reuse already-downloaded images unless URL/content changed.

A typical run after a small edit touches 1–2 files in well under a second; Syncthing ships only diffs.

### 6.4 Image pipeline (offline-first core)

Handles `![alt](https://…)`, raw `<img src="https://…">`, and local `![[attachment.png]]` (copied, not downloaded).

| Concern | Default |
|---|---|
| Cache | shared `/vault.kindle/assets/` (one place → dedup) |
| Dedup | content-hash → stored once |
| Fetch | thread pool (4), 15s timeout, 3× exp. backoff retries |
| **Optimize for PW7 e-ink** | downscale max-width **~1000px**, **grayscale**, JPEG recompress q≈70 (visually lossless on grayscale e-ink) |
| Format normalize | convert unsupported formats (WebP…) → JPEG/PNG |
| Failure | placeholder `[image unavailable: url]` inline + logged + recorded; retried next run |
| Size guard | skip images > N MB (configurable) |

### 6.5 Output layout

`/vault.kindle/` mirrors the vault folder tree, content only (`.obsidian/`, templates, etc. excluded via config):

```
/vault.kindle/
├── Notes/Projects/koreader-design.html   ← from koreader-design.md
├── Notes/Inbox/clipped-2026-07-10.html
├── Daily/2026-07-13.html
└── assets/                               ← shared, dedup'd, optimized
    └── .md2kindle/manifest.json
```

- `foo.md` → `foo.html` at the same relative path.
- Image refs rewritten to correct relative paths into root `assets/` (computed per file).
- `[[wikilinks]]` rewritten to relative `.html` links (§8).
- Atomic writes so Syncthing never ships a half-written file.

### 6.6 Markdown → HTML + e-ink CSS

- **Python-Markdown** with extensions: `tables`, `fenced_code`, `attr_list`, `toc`, `admonition`. Post-process HTML to rewrite `src`/`href`.
- Wikilink rewriting is a **code-aware preprocessor** (runs before rendering; skips code fences, inline code, comments) that emits standard `[text](relpath.html#anchor)` links.
- Inject `kindle.css`: serif body, comfortable line-height, paragraph spacing, `img { max-width: 100% }`, subtle code-block background. User-overridable.

## 7. Unit ② — `obsidian.koplugin` (Lua)

Standard KOReader plugin (`_meta.lua` + `main.lua`, `WidgetContainer:extend`); attaches menu items in both reader (ReaderUI) and file-manager contexts. Shared state: **vault root** + **current file** (`self.ui.document.file`).

### 7.1 `linkhandler` — cross-file navigation

- **Hook:** `self.ui.link:registerScheme("")` on init (the "empty scheme" path from PRs #11889/#12019) so relative/schemeless links route to us instead of "Invalid or external link".
- **On tap:**
  - same-file `#slug` → let CREngine's native anchor jump handle it.
  - cross-file → resolve against current document dir → absolute path → open via KOReader document-switch API (e.g., `ReaderUI:showReader(path)`); push current path onto back-stack; jump to `#anchor` on load if present (CREngine xpointer).
  - missing target → `InfoMessage` "Note not found" (defensive).
- **Back-stack:** plugin-owned `[path…]` stack (KOReader has no cross-document back). **Back** triggered via a **top-menu entry**. Pop + reopen previous note, restoring scroll where feasible; disabled when empty.

### 7.2 `vaultbrowser` — collapsible tree

```
┌─ Vault ────────────────────────── [⤿collapse] [⤢expand] [◎focus] ─┐
│  ▾ Notes                                                           │
│    ▾ Projects                                                      │
│      ▸ KOReader plugin                                             │
│      ● koreader-design            ← current file (highlighted)     │
│    ▸ Reading                                                       │
│  ▸ Daily notes                                                     │
└────────────────────────────────────────────────────────────────────┘
   ▾ expanded   ▸ collapsed   ● open file   · file
```

- Walks vault root once (cached in memory); tree of folders + `.html` files (non-content hidden).
- Render: custom widget on `Menu`/`VerticalGroup`, indented with `▸/▾`; tap folder toggles; tap file opens + marks current.
- Toolbar: **collapse all**, **expand all**, **focus current** (expand ancestor chain of the open file, scroll to + highlight).
- Labels: note title (first `<h1>`, else filename), `.html` stripped.
- Expand state persisted in plugin settings.
- Entry point: "Vault browser" under the top menu (Tools), from reader or file manager.
- Read-only — no file operations.

### 7.3 Settings

Vault root path, sort order (folders-first alpha), show/hide `assets`. First-run prompts for the vault root.

## 8. Wikilink resolution spec

### 8.1 Supported forms

| Form | Resolves to | Phase 1 |
|---|---|---|
| `[[Note]]` | basename anywhere in vault | ✅ |
| `[[Folder/Note]]` | partial path | ✅ |
| `[[Note\|alias]]` | alias = display text | ✅ |
| `[[Note#Heading]]` | note + in-page anchor | ✅ (via `toc` heading IDs) |
| `[[#Heading]]` | same-note anchor | ✅ |
| `[[Note^blockid]]` | block reference | ⚠️ link to note top; block anchor ignored |
| `[text](Note.md)` / `[text](./Folder/Note.md)` | standard md links | ✅ rewritten `.md`→`.html` |
| `![[image.png]]` | local attachment | ✅ via image pipeline |
| `![[Note]]` | note transclusion | ⚠️ **placeholder** `[transclusion: Note]` (not inlined; user doesn't use these) |

### 8.2 Algorithm

1. **Build index** at scan time: `basename → [vault-relative paths]` (case-insensitive).
2. **Parse** `[[ TARGET ]]` into path `P`, optional anchor `A` (`#heading` / `^block`), optional alias.
3. **Candidates:** if `P` has `/` → notes whose vault-relative path ends with `P`; else notes whose basename == `P`.
4. **Resolve:** 1 match → use it; N matches → shortest vault-relative path wins (Obsidian-compatible) + log warning; 0 matches → broken.
5. **Emit:** broken → `<span class="broken-link">P</span>` (dimmed, non-clickable) + logged; resolved → `<a href="<relpath>.html[#slug]">alias-or-basename</a>`.

Display text: `[[Folder/Note]]` → "Note" (basename); `[[Note\|My Text]]` → "My Text".

## 9. Edge cases & error handling

**Converter:** never rewrite `[[…]]` inside code fences/inline code/comments; strip YAML frontmatter (default); URL-encode hrefs for spaces/unicode; UTF-8 end-to-end; duplicate basenames disambiguated + logged; tags `#tag` and math `$$…$$` left as plain text; code blocks render monospace (no JS highlighting).

**Images:** unsupported format → converted; oversized → skipped + placeholder; fetch fail → placeholder + logged + retried next run; missing local attachment → placeholder.

**Sync/state:** source deleted → orphan output removed; mid-edit during a pass → idempotent (next pass wins); atomic writes.

**On-device:** broken/missing target → `InfoMessage`; empty back-stack → Back disabled; large vault → scrolled list.

**Philosophy:** fail soft, log loudly, never corrupt output; every run safe to re-run.

## 10. Repo layout, languages, dev/test

```
obsidian-kindle/
├── docs/                      # phase-1-notes.md, reference-prompt.md, design.md
├── converter/                 # md2kindle (Python 3.10+)
│   ├── md2kindle/             # cli, pipeline, manifest, resolver, images, renderer, layout, config
│   ├── tests/                 # pytest + fixture vaults (+ fake image server)
│   ├── pyproject.toml
│   └── md2kindle.toml.example
└── plugin/                    # obsidian.koplugin (Lua)
    └── obsidian.koplugin/
        ├── _meta.lua  main.lua  linkhandler.lua  vaultbrowser.lua
```

- **Converter deps:** `markdown`, `Pillow`, `httpx`, `watchdog` (optional), stdlib `tomllib`. Dev: `pytest`, `ruff`. Fully unit-testable on the dev box; no Kindle needed.
- **Plugin:** pure Lua, **zero external deps** — drop into `koreader/plugins/`.
- **Plugin dev loop:** iterate with `kodev` (KOReader as a desktop app on Linux), then deploy the `.koplugin` folder via USB or Syncthing.
- **Converter deploy:** `pip install` the package → Task Scheduler runs `python -m md2kindle sync` every ~5 min.

## 11. To verify during implementation

API specifics to confirm against the KOReader version on the device:
- Exact `ReaderLink:registerScheme("")` callback signature/event (PRs #11889, #12019).
- Document-open API (`ReaderUI:showReader` vs `switchDocument`-style handler) for cross-file navigation.
- CREngine anchor-on-open behavior (jumping to `#slug` via xpointer when opening a new file).
- Best hook point for adding the top-menu "Vault browser" / "Back" entries in both reader and file-manager contexts.

## 12. Decisions log

| Decision | Choice | Rationale (alternatives considered) |
|---|---|---|
| Scope | Rendering plugin + converter; sync via existing tools | Sync already solvable with `kosyncthing_plus` + Syncthing bridge |
| Network model | Offline-first; images pre-fetched on bridge | User reads mostly offline on old/slow hardware |
| Architecture | **A**: bridge→HTML + small nav plugin | (C) single EPUB was simpler but loses file-manager UX + needs full rebuild; (B) pure on-device plugin fights the hardware |
| Converter trigger | Scheduled incremental (Task Scheduler ~5 min) | Windows file-watchers drop events under git churn; lag is irrelevant here |
| Image optimization | Downscale ~1000px + grayscale + JPEG | Visually lossless on e-ink; big storage/render win on PW7 |
| Transclusions | Placeholder (not inlined) | User doesn't use them; keeps converter simple, defers recursion |
| Back navigation | Top-menu button | Paperwhite is touchscreen-only; menu is reliable and discoverable |
| Vault browser | Custom collapsible tree widget | KOReader's file manager is a flat drill-in list; no off-the-shelf tree |
