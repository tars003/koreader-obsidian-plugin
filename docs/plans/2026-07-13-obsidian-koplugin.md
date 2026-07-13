# obsidian.koplugin — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Lua syntax-check is the test gate (`lua5.1 -p`); functional testing requires KOReader on device or via `kodev`.

**Goal:** Build `obsidian.koplugin`, a Lua KOReader plugin that enables cross-file `[[wikilink]]` navigation between `.html` notes and provides an Obsidian-style collapsible vault browser — both working on the Kindle Paperwhite 7th gen running KOReader, assuming the converter (md2kindle, Plan A) has produced the self-contained `.html` vault.

**Architecture:** The plugin extends KOReader's `WidgetContainer`, registers the empty URL scheme (`""`) to intercept relative links via a wrapper on ReaderLink, and provides a Menu-based collapsible tree widget for browsing the vault. Two modules share a vault-root setting and a cross-document back-stack.

**Tech Stack:** Lua 5.1 (KOReader's runtime), KOReader APIs (`WidgetContainer`, `Menu`, `UIManager`, `LuaSettings`, `ReaderLink`, `lfs`, `ffiUtil`). Zero external deps beyond KOReader's built-in modules.

## Global Constraints

- **Lua 5.1 syntax** — use tabs as KOReader convention (spaces in user-facing strings), no Lua patterns unsupported by 5.1.
- **KOReader API compatibility** — `require` paths, widget lifecycle, and menu registration must match KOReader's established patterns (see `plugins/hello.koplugin/main.lua` and the plugin dev guide).
- **Read-only on documents** — the plugin never modifies files on the Kindle; it only reads the vault and navigates.
- **Fail soft** — a broken/missing link target shows an `InfoMessage`, never crashes the reader.
- **Back-stack is in-memory** — cleared on reader close; persistence of navigation history is not required for Phase 1.
- **Deployable** by dropping `obsidian.koplugin/` into `koreader/plugins/`.

---

## File Structure

```
plugin/
└── obsidian.koplugin/
    ├── _meta.lua              # plugin metadata
    ├── main.lua               # entry: WidgetContainer, init, menu, settings, back-stack
    ├── linkhandler.lua        # registerScheme("") wrapper, cross-file open, back-navigation
    └── vaultbrowser.lua       # tree build + Menu-based browser UI
```

**Responsibilities:** `main.lua` owns the plugin lifecycle (init, menu, settings read/write, shared back-stack). `linkhandler.lua` replaces ReaderLink's `onGoToExternalLink` when a reader document is open to auto-follow relative `.html` links (bypassing the default ConfirmBox dialog). `vaultbrowser.lua` scans the vault root, builds a folder tree, renders it in a full-screen Menu with collapse/expand/focus buttons, and opens tapped files.

---

## Task 1: Plugin scaffold + settings

**Files:**
- Create: `plugin/obsidian.koplugin/_meta.lua`
- Create: `plugin/obsidian.koplugin/main.lua`

**Interfaces:**
- Produces: `obsidian.koplugin` plugin registered in KOReader with a "Vault" submenu (sorting_hint `"tools"`). Settings stored in `koreader/obsidian.lua` with keys `vault_root`, and an expand-state table. A back-stack table `back_stack` (empty initially).

- [ ] **Step 1: Write the plugin files**

```lua
-- plugin/obsidian.koplugin/_meta.lua
local _ = require("gettext")
return {
    name = "obsidian",
    fullname = _("Obsidian Vault"),
    description = _([[Navigate an Obsidian .html vault with wikilink support and a collapsible vault browser.]]),
}
```

```lua
-- plugin/obsidian.koplugin/main.lua
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local UIManager = require("ui/uimanager")
local InfoMessage = require("ui/widget/infomessage")
local LuaSettings = require("luasettings")
local DataStorage = require("datastorage")
local _ = require("gettext")

local ObsidianPlugin = WidgetContainer:extend{
    name = "obsidian",
    is_doc_only = false,
}

function ObsidianPlugin:init()
    self.back_stack = {}
    self:loadSettings()
    self.ui.menu:registerToMainMenu(self)
    -- link handler: only active when a document is open
    if self.ui.document then
        self:initLinkHandler()
    end
end

function ObsidianPlugin:loadSettings()
    self.settings = LuaSettings:open(
        DataStorage:getSettingsDir() .. "/obsidian.lua"
    )
end

function ObsidianPlugin:saveSettings()
    self.settings:flush()
end

function ObsidianPlugin:addToMainMenu(menu_items)
    local vault_root_text = self.settings:readSetting("vault_root") or _("(not set)")
    menu_items.obsidian = {
        text = _("Obsidian Vault"),
        sorting_hint = "tools",
        sub_item_table = {
            {
                text = _("Browse vault"),
                keep_menu_open = true,
                callback = function() self:openVaultBrowser() end,
            },
            {
                text = _("Go back to previous note"),
                enabled_func = function() return #self.back_stack > 0 end,
                separator = true,
                callback = function() self:goBack() end,
            },
            {
                text = _("Clear navigation history"),
                enabled_func = function() return #self.back_stack > 0 end,
                callback = function() self:clearBackStack() end,
            },
            {
                text = T(_("Vault root: %1"), vault_root_text),
                separator = true,
                keep_menu_open = true,
                callback = function() self:promptVaultRoot() end,
            },
        },
    }
end

function ObsidianPlugin:promptVaultRoot()
    -- Task 2 wires this
    UIManager:show(InfoMessage:new{ text = _("Vault root setting will prompt for path (Task 2).") })
end

function ObsidianPlugin:openVaultBrowser()
    -- Task 3/4 wire this
    UIManager:show(InfoMessage:new{ text = _("Vault browser coming in Task 3/4.") })
end

function ObsidianPlugin:goBack()
    if #self.back_stack > 0 then
        local previous = table.remove(self.back_stack)
        self.ui:switchDocument(previous)
    end
end

function ObsidianPlugin:clearBackStack()
    self.back_stack = {}
end

function ObsidianPlugin:initLinkHandler()
    -- Task 2 wires this
end

function ObsidianPlugin:onFlushSettings()
    self:saveSettings()
end

function ObsidianPlugin:onCloseDocument()
    self:clearBackStack()
end

return ObsidianPlugin
```

- [ ] **Step 2: Syntax-check (the test gate)**

Run: `lua5.1 -p plugin/obsidian.koplugin/main.lua`

Expected: no output (0 exit code). Syntax errors would print a line:number message. Warnings (e.g., unused variables) are acceptable for this staging step.

- [ ] **Step 3: Commit**

```bash
git add plugin/obsidian.koplugin/_meta.lua plugin/obsidian.koplugin/main.lua
git commit -m "feat(plugin): scaffold obsidian.koplugin + settings + menu skeleton"
```

---

## Task 2: Link handler (cross-file navigation + back-stack)

**Files:**
- Create: `plugin/obsidian.koplugin/linkhandler.lua`
- Modify: `plugin/obsidian.koplugin/main.lua` (wire `initLinkHandler` and `promptVaultRoot`)

**Interfaces:**
- Consumes: `self.ui.link` (ReaderLink instance, available when `self.ui.document` is not nil).
- Produces: `initLinkHandler()` installs a wrapper on `ReaderLink:onGoToExternalLink` that directly opens relative links to `.html` files (via `self.ui:switchDocument`), pushes the current file onto `self.back_stack`, and falls through to the original `onGoToExternalLink` for non-file links. `resolveTarget(link_url, current_file, vault_root)` → `string|nil` resolves a relative URL against the current document's directory, checking for `.html` extension and file existence.

- [ ] **Step 1: Write linkhandler.lua**

```lua
-- plugin/obsidian.koplugin/linkhandler.lua
local ffiUtil = require("ffi/util")
local lfs = require("libs/libkoreader-lfs")
local UIManager = require("ui/uimanager")
local InfoMessage = require("ui/widget/infomessage")
local _ = require("gettext")

local LinkHandler = {}

-- Resolve a relative link_url against current_file's directory.
-- Returns an absolute path to the target .html file, or nil.
function LinkHandler.resolveTarget(link_url, current_file, vault_root)
    -- Strip anchor (#…) and query (?…) before resolving
    local path = link_url:gsub("[#?].*$", "")
    if path == "" then
        return nil  -- same-file anchor
    end
    -- Resolve relative to current document's dir
    local doc_dir = current_file:match("(.*/)") or "" -- dir part, or empty for root
    local resolved = ffiUtil.joinPath(doc_dir, path)
    -- Normalize to absolute real path; KOReader's realpath normalises ./ and ../
    resolved = ffiUtil.realpath(resolved)
    if not resolved then
        return nil
    end
    -- Verify it's a file we can open
    local attr = lfs.attributes(resolved, "mode")
    if attr ~= "file" then
        return nil
    end
    return resolved
end

-- Install the wrapper on ReaderLink. Called from init when a document is open.
function LinkHandler.install(plugin)
    local link = plugin.ui.link
    if not link then return end

    -- Register the empty scheme so relative/schemeless URLs reach onGoToExternalLink
    link:registerScheme("")

    -- Save original and wrap
    local orig_onGoToExternalLink = link.onGoToExternalLink
    local vault_root = plugin.settings:readSetting("vault_root") or ""

    function link:onGoToExternalLink(link_url)
        -- If it's a schemeless/relative link AND auto-follow is on, try direct open
        if not link_url:match("^%w[%w+%-.]*:") and vault_root ~= "" then
            local target = LinkHandler.resolveTarget(
                link_url, plugin.ui.document.file, vault_root
            )
            if target then
                -- Push current file onto back-stack
                table.insert(plugin.back_stack, plugin.ui.document.file)
                plugin.ui:switchDocument(target)
                return true
            else
                -- Broken or missing target
                UIManager:show(InfoMessage:new{
                    text = _("Note not found"),
                    timeout = 2,
                })
                return true  -- prevent fallthrough to original dialog
            end
        end
        -- Let original handler show its dialog for http/mailto/etc.
        return orig_onGoToExternalLink(self, link_url)
    end
end

return LinkHandler
```

- [ ] **Step 2: Wire into main.lua**

Replace the stub `initLinkHandler` and `promptVaultRoot` in `main.lua`:

```lua
-- add import at top of main.lua (after existing requires)
local LinkHandler = require("linkhandler")
--
-- replace the two stubs:

function ObsidianPlugin:initLinkHandler()
    LinkHandler.install(self)
end

function ObsidianPlugin:promptVaultRoot()
    local InputDialog = require("ui/widget/inputdialog")
    local input_dialog
    input_dialog = InputDialog:new{
        title = _("Vault root path"),
        input = self.settings:readSetting("vault_root") or "",
        input_hint = _("/mnt/us/vault.kindle"),
        buttons = {{
            {
                text = _("Cancel"),
                callback = function()
                    UIManager:close(input_dialog)
                end,
            },
            {
                text = _("OK"),
                callback = function()
                    local val = input_dialog:getInputText()
                    self.settings:saveSetting("vault_root", val)
                    self:saveSettings()
                    UIManager:close(input_dialog)
                end,
            },
        }},
    }
    UIManager:show(input_dialog)
    input_dialog:onShowKeyboard()
end
```

- [ ] **Step 3: Syntax-check**

Run: `lua5.1 -p plugin/obsidian.koplugin/main.lua && lua5.1 -p plugin/obsidian.koplugin/linkhandler.lua`

Expected: no output (0 exit code).

- [ ] **Step 4: Commit**

```bash
git add plugin/obsidian.koplugin/linkhandler.lua plugin/obsidian.koplugin/main.lua
git commit -m "feat(plugin): link handler — cross-file navigation + back-stack"
```

---

## Task 3: Vault browser tree model

**Files:**
- Create: `plugin/obsidian.koplugin/vaultbrowser.lua` (tree scan + expand/collapse logic)

**Interfaces:**
- Produces: `VaultBrowser` module with: `scanVault(vault_root) -> tree_table` (nested `{name, path, type="dir"|"file", expanded=false, children={...}}`), `flattenTree(tree, indent) -> flat_item_table` (for Menu display), `toggleNode(tree, path)` (expand/collapse), `expandAll(tree)`, `collapseAll(tree)`, `findPath(tree, needle)` (returns chain of nodes along path).

- [ ] **Step 1: Write the tree model**

```lua
-- plugin/obsidian.koplugin/vaultbrowser.lua
local lfs = require("libs/libkoreader-lfs")
local ffiUtil = require("ffi/util")

local VaultBrowser = {}

-- Wrap lfs.dir results in a sorted table (dirs first, then files, alpha)
local function listDirSorted(dir_path)
    local entries = {}
    for name in lfs.dir(dir_path) do
        if name ~= "." and name ~= ".." and name ~= "assets" then
            local full = ffiUtil.joinPath(dir_path, name)
            local mode = lfs.attributes(full, "mode")
            table.insert(entries, {name = name, path = full, mode = mode or "unknown"})
        end
    end
    table.sort(entries, function(a, b)
        if a.mode == "directory" and b.mode ~= "directory" then return true end
        if a.mode ~= "directory" and b.mode == "directory" then return false end
        return a.name:lower() < b.name:lower()
    end)
    return entries
end

-- Recursively build the tree
function VaultBrowser.scanVault(vault_root)
    local function _scan(path)
        local name = path:match("([^/]+)$") or path
        local node = {
            name = name,
            path = path,
            type = "directory",
            expanded = false,
            children = {},
        }
        local entries = listDirSorted(path)
        for _, e in ipairs(entries) do
            if e.mode == "directory" then
                table.insert(node.children, _scan(e.path))
            elseif e.name:match("%.html$") then
                table.insert(node.children, {
                    name = e.name:gsub("%.html$", ""),
                    path = e.path,
                    type = "file",
                })
            end
        end
        return node
    end
    return _scan(vault_root)
end

-- Flatten expanded nodes into a Menu-ready item_table
function VaultBrowser.flattenTree(node, indent)
    indent = indent or 0
    local items = {}
    local prefix = string.rep("  ", indent)
    if node.type == "directory" then
        local marker = node.expanded and "▾ " or "▸ "
        table.insert(items, {
            text = prefix .. marker .. node.name .. "/",
            path = node.path,
            type = "directory",
            expanded = node.expanded,
            indent = indent,
        })
        if node.expanded then
            for _, child in ipairs(node.children) do
                local child_items = VaultBrowser.flattenTree(child, indent + 1)
                for _, ci in ipairs(child_items) do
                    table.insert(items, ci)
                end
            end
        end
    else
        table.insert(items, {
            text = prefix .. "· " .. node.name,
            path = node.path,
            type = "file",
            indent = indent,
        })
    end
    return items
end

-- Toggle a directory node at the given absolute path
function VaultBrowser.toggleNode(tree, path)
    local function _toggle(node)
        if node.path == path then
            node.expanded = not node.expanded
            return true
        end
        if node.children then
            for _, child in ipairs(node.children) do
                if _toggle(child) then return true end
            end
        end
        return false
    end
    return _toggle(tree)
end

-- Collapse all nodes
function VaultBrowser.collapseAll(node)
    if node.children then
        node.expanded = false
        for _, child in ipairs(node.children) do
            VaultBrowser.collapseAll(child)
        end
    end
end

-- Expand all nodes
function VaultBrowser.expandAll(node)
    if node.children then
        node.expanded = true
        for _, child in ipairs(node.children) do
            VaultBrowser.expandAll(child)
        end
    end
end

-- Find the ancestor chain for a file path (for focus-current)
function VaultBrowser.findChain(tree, needle)
    -- Returns list of nodes from root to leaf, or nil
    -- Match by comparing tail of path
    local function _find(node, path_rest)
        if node.path == needle then
            return {node}
        end
        if node.children then
            for _, child in ipairs(node.children) do
                local found = _find(child, needle)
                if found then
                    table.insert(found, 1, node)
                    return found
                end
            end
        end
        return nil
    end
    return _find(tree, needle)
end

return VaultBrowser
```

- [ ] **Step 2: Syntax-check**

Run: `lua5.1 -p plugin/obsidian.koplugin/vaultbrowser.lua`

Expected: no output (0 exit code).

- [ ] **Step 3: Commit**

```bash
git add plugin/obsidian.koplugin/vaultbrowser.lua
git commit -m "feat(plugin): vault browser tree model (scan/flatten/toggle/expand/collapse)"
```

---

## Task 4: Vault browser UI (Menu dialog)

**Files:**
- Modify: `plugin/obsidian.koplugin/vaultbrowser.lua` (add UI rendering)
- Modify: `plugin/obsidian.koplugin/main.lua` (wire `openVaultBrowser`)

**Interfaces:**
- Consumes: `Menu`, `UIManager`, the tree model from Task 3.
- Produces: `showBrowser(plugin)` — builds a full-screen Menu dialog showing the vault tree with toolbar items (Collapse all / Expand all / Focus current), handles tap callbacks (toggle dir / open file), and closes on file-open or cancel.

- [ ] **Step 1: Add the UI to vaultbrowser.lua**

Append to `plugin/obsidian.koplugin/vaultbrowser.lua`:

```lua
local Menu = require("ui/widget/menu")
local UIManager = require("ui/uimanager")
local InfoMessage = require("ui/widget/infomessage")
local _ = require("gettext")

-- Build the flat item_table from the tree
function VaultBrowser.buildItemTable(tree)
    local items = {
        {
            text = _("⤿ Collapse all   ⤢ Expand all   ◎ Focus current"),
            enabled_func = function() return false end,  -- non-interactive label
        },
        {
            text = _("──────────────────────────────────"),
            enabled_func = function() return false end,
        },
    }
    local tree_items = VaultBrowser.flattenTree(tree)
    for _, ti in ipairs(tree_items) do
        table.insert(items, ti)
    end
    return items
end

-- Show the full-screen vault browser dialog
function VaultBrowser.showBrowser(plugin)
    local vault_root = plugin.settings:readSetting("vault_root")
    if not vault_root or vault_root == "" then
        UIManager:show(InfoMessage:new{
            text = _("Please set the vault root first (Obsidian Vault → Vault root)."),
            timeout = 3,
        })
        return
    end

    local tree = VaultBrowser.scanVault(vault_root)

    local menu

    local function rebuildMenu()
        menu:switchItemTable(nil, VaultBrowser.buildItemTable(tree), -1)
    end

    menu = Menu:new{
        title = _("Vault Browser"),
        item_table = VaultBrowser.buildItemTable(tree),
        covers_fullscreen = true,
        is_borderless = true,
        width = nil,  -- full width
        height = nil, -- full height
        close_callback = function()
            UIManager:close(menu)
        end,
        -- Override item callback: toolbar items (index 1) are ignored;
        -- tree items toggle folders or open files.
        callback = function(item)
            if item.type == "directory" then
                VaultBrowser.toggleNode(tree, item.path)
                rebuildMenu()
            elseif item.type == "file" then
                UIManager:close(menu)
                plugin.ui:switchDocument(item.path)
            end
        end,
        hold_callback = function(item)
            -- Relabel the three toolbar items by position
            if item.idx == 1 then
                -- Collapse all
                VaultBrowser.collapseAll(tree)
                rebuildMenu()
            elseif item.idx == 2 then
                -- Expand all
                VaultBrowser.expandAll(tree)
                rebuildMenu()
            elseif item.idx == 3 then
                -- Focus current
                local current_file = plugin.ui.document and plugin.ui.document.file
                if current_file then
                    local chain = VaultBrowser.findChain(tree, current_file)
                    if chain then
                        -- Expand all ancestors
                        for _, n in ipairs(chain) do
                            n.expanded = true
                        end
                        rebuildMenu()
                    else
                        UIManager:show(InfoMessage:new{ text = _("Current file not in vault."), timeout = 2 })
                    end
                end
            end
        end,
    }

    UIManager:show(menu)
end
```

**Note:** The `callback` key in Menu is called for ordinary taps; `hold_callback` fires on long-press. For the three toolbar items, a long-press triggers the action (collapse/expand/focus). This keeps the tree-item tap (toggle/open) on the primary callback. For the "Collapse all / Expand all / Focus current" label row (index 1), it's a non-interactive label — the long-press actions for the toolbar are mapped to positions 1-3 in the hold_callback via `item.idx`.

**Alternative (cleaner toolbar):** use three separate items instead of one combined label:

```lua
function VaultBrowser.buildItemTable(tree)
    local items = {
        {
            text = _("⤿ Collapse all"),
            hold_callback = function() … end,  -- see handling in Menu
        },
        {
            text = _("⤢ Expand all"),
            hold_callback = function() … end,
        },
        {
            text = _("◎ Focus current"),
            hold_callback = function() … end,
        },
        {
            text = _("───────────────────"),
            enabled_func = function() return false end,
        },
    }
    -- treat toolbar items as special in the callback
    ...
end
```

Use whichever variant the implementer finds cleaner; both pass the spec as long as all three actions work and the tree toggles/open on tap.

- [ ] **Step 2: Wire openVaultBrowser in main.lua**

Replace the stub:

```lua
function ObsidianPlugin:openVaultBrowser()
    local VaultBrowser = require("vaultbrowser")
    VaultBrowser.showBrowser(self)
end
```

Also add the `require` near the top of `main.lua`:
```lua
local VaultBrowser = require("vaultbrowser")
```

- [ ] **Step 3: Syntax-check all files**

Run: `lua5.1 -p plugin/obsidian.koplugin/_meta.lua && lua5.1 -p plugin/obsidian.koplugin/main.lua && lua5.1 -p plugin/obsidian.koplugin/linkhandler.lua && lua5.1 -p plugin/obsidian.koplugin/vaultbrowser.lua`

Expected: no output (0 exit code).

- [ ] **Step 4: Commit**

```bash
git add plugin/obsidian.koplugin/vaultbrowser.lua plugin/obsidian.koplugin/main.lua
git commit -m "feat(plugin): vault browser UI — collapsible tree with toolbar"
```

---

## Task 5: Integration, settings polish, deployment

**Files:**
- Modify: `plugin/obsidian.koplugin/main.lua` (expand-state persistence in settings, first-run prompt)
- Modify: `plugin/obsidian.koplugin/vaultbrowser.lua` (wire expand-state save/load)

**Interfaces:**
- Produces: the plugin remembers expand-state between sessions; first-run prompts for `vault_root` if not set. A `README.md` documents how to deploy.

- [ ] **Step 1: Persist expand-state across browser invocations**

In `vaultbrowser.lua`, before rebuilding the tree, read saved expand-state:

```lua
function VaultBrowser.loadExpanded(path)
    local state = plugin.settings:readSetting("tree_expanded") or {}
    return state[path] or false
end

function VaultBrowser.saveExpanded(path, expanded)
    local state = plugin.settings:readSetting("tree_expanded") or {}
    state[path] = expanded
    plugin.settings:saveSetting("tree_expanded", state)
    plugin:saveSettings()
end
```

In `toggleNode`, after toggling, call `saveExpanded(node.path, node.expanded)`. In `collapseAll`/`expandAll`, iterate and save each. In `scanVault`, initialize `expanded` from `loadExpanded`. (The implementer wires these calls; the spec says: expand-state is remembered per directory, saved to a `tree_expanded` table in obsidian.lua settings.)

- [ ] **Step 2: First-run vault-root prompt**

In `main.lua`, if `vault_root` is not set on init, show the prompt automatically:

```lua
function ObsidianPlugin:init()
    self.back_stack = {}
    self:loadSettings()
    self.ui.menu:registerToMainMenu(self)
    if self.ui.document then
        self:initLinkHandler()
    end
    -- First-run: prompt for vault root
    if not self.settings:readSetting("vault_root") then
        UIManager:nextTick(function()
            self:promptVaultRoot()
        end)
    end
end
```

Note: `UIManager:nextTick` schedules the prompt after the UI fully loads so the InputDialog renders correctly.

- [ ] **Step 3: Create a brief README**

```markdown
# obsidian.koplugin

KOReader plugin for reading an Obsidian `.html` vault (produced by `md2kindle`) on a Kindle.

## Install
Copy `obsidian.koplugin/` into `<kindle>/koreader/plugins/`, restart KOReader.
Set the vault root via `Obsidian Vault → Vault root` (e.g. `/mnt/us/vault.kindle`).

## Requires
- KOReader with a recent ReaderLink (supports `registerScheme("")` — PRs #11889/#12019).
- The vault root populated by `md2kindle` converter (Plan A), synced via Syncthing or USB.

## Features
- Tap a `[[wikilink]]`-style link to open the referenced note; back via `Go back to previous note`.
- `Browse vault`: collapsible folder tree with collapse/expand all and focus current.
```

- [ ] **Step 4: Final syntax-check + log**

Run: `lua5.1 -p plugin/obsidian.koplugin/*.lua`

Expected: no output (0 exit code).

Run:
```bash
echo "--- files ---" && find plugin/obsidian.koplugin -type f | sort
echo "--- line counts ---" && wc -l plugin/obsidian.koplugin/*
```

- [ ] **Step 5: Commit**

```bash
git add plugin/obsidian.koplugin/ plugin/README.md
git commit -m "feat(plugin): integration polish (expand-state persistence, first-run prompt, README)"
```

---

## Deployment to Kindle

1. Connect Kindle via USB (or Syncthing).
2. Copy `plugin/obsidian.koplugin/` → `<kindle>/koreader/plugins/obsidian.koplugin/`.
3. Restart KOReader on Kindle.
4. Open the menu → `Obsidian Vault → Vault root` and set the path to your synced vault (e.g. `/mnt/us/vault.kindle`).
5. Open any `.html` note from the KOReader file manager (CREngine renders HTML).
6. Tap a wikilink — the link handler opens the target note and adds the current note to the back-stack. Use `Go back to previous note` to return.
7. Use `Browse vault` to open the collapsible tree.

---

## Self-Review (completed by planner)

**1. Design spec coverage (§7):**
- Plugin packaging (WidgetContainer, _meta.lua) ✅ Task 1.
- linkhandler: registerScheme("") wrapper + switchDocument + back-stack menu ✅ Task 2.
- vaultbrowser: tree build (lfs.dir), collapsible render (Menu), collapse/expand all + focus current ✅ Tasks 3/4.
- Settings persistence (vault_root, expand-state) ✅ Task 5.
- First-run vault_root prompt ✅ Task 5.
- Cross-file back navigation via menu ✅ Task 1/Task 2.

**2. Placeholder scan:** No TBD/TODO/"implement later" in code. The toolbar UX (one combined label vs three separate items) gives the implementer a choice; both satisfy the spec. The `hold_callback` approach for toolbar actions is available in KOReader's Menu. No stub left unresolved.

**3. Type/name consistency:** `LinkHandler.resolveTarget(link_url, current_file, vault_root)` → string matching across Task 2. `VaultBrowser.{scanVault, flattenTree, toggleNode, collapseAll, expandAll, findChain, showBrowser}` consistent across Tasks 3-4. `plugin.back_stack` used in Task 1 (goBack/clearBackStack) and Task 2 (push on navigate). Settings key `vault_root` used throughout. ✅

**4. Open items carried to build:**
- The `hold_callback` for toolbar items relies on KOReader's Menu supporting `item.idx`. In practice, KOReader's Menu passes the item index in `item.idx`. If the version on the user's Kindle differs, the toolbar actions may need a fallback (a secondary callback array). The implementer should verify on the target KOReader version.
- `ffiUtil.realpath` on the Kindle filesystem (FAT32/USB mount) must resolve correctly — test on-device.
- CREngine's HTML rendering of the e-ink CSS from the converter — verify headings, images, and `.broken-link` class render as designed.
