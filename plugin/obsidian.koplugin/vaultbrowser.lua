-- plugin/obsidian.koplugin/vaultbrowser.lua
local lfs = require("libs/libkoreader-lfs")
local ffiUtil = require("ffi/util")
local Menu = require("ui/widget/menu")
local UIManager = require("ui/uimanager")
local InfoMessage = require("ui/widget/infomessage")
local _ = require("gettext")

local VaultBrowser = {}

-- Wrap lfs.dir results in a sorted table (dirs first, then files, alpha)
local function listDirSorted(dir_path)
    local entries = {}
    for name in lfs.dir(dir_path) do
        if name ~= "." and name ~= ".." and name ~= "assets" then
            local full = ffiUtil.joinPath(dir_path, name)
            local mode = lfs.attributes(full, "mode")
            if mode == "directory" then
                -- known directory; will be recursed
            elseif mode == "file" then
                -- known file; caller filters by .html
            else
                -- lfs.attributes failed (e.g. Kindle FAT32 quirks).
                -- Use the name's extension as a last-resort classifier.
                -- Never recurse on a non-explicit directory — that path
                -- is what causes the stack overflow on Kindle.
                if name:match("%.html?$") then
                    mode = "file"
                end
                -- else: drop the entry (don't recurse, don't add)
            end
            if mode then
                table.insert(entries, {name = name, path = full, mode = mode})
            end
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
    local visited = {}
    local function _scan(path)
        if visited[path] then
            return nil  -- cycle protection (e.g. self-loop symlink on PC, or repeated path on Kindle)
        end
        visited[path] = true
        local name = path:match("([^/]+)$") or path
        local node = {
            name = name,
            path = path,
            type = "directory",
            expanded = false,
            children = {},
        }
        local entries
        do
            local ok_list, result = pcall(listDirSorted, path)
            if not ok_list then
                -- Could not list this directory (permission denied, etc.).
                -- Return a stub node with no children so the tree still builds.
                -- The outer pcall in showBrowser will catch the rare case where
                -- even the root fails to list.
                io.stderr:write("vaultbrowser: listDirSorted failed for "
                    .. tostring(path) .. ": " .. tostring(result) .. "\n")
                return node
            end
            entries = result
        end
        for _, e in ipairs(entries) do
            if e.mode == "directory" then
                local child = _scan(e.path)
                if child then
                    table.insert(node.children, child)
                end
            elseif e.name:match("%.html?$") then
                table.insert(node.children, {
                    name = e.name:gsub("%.html?$", ""),
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
    local function _find(node)
        if node.path == needle then
            return {node}
        end
        if node.children then
            for _, child in ipairs(node.children) do
                local found = _find(child)
                if found then
                    table.insert(found, 1, node)
                    return found
                end
            end
        end
        return nil
    end
    return _find(tree)
end

-- Build the flat item_table from the tree.
-- Each item gets its own callback (no Menu-level dispatch).
-- To refresh state after an action, callbacks call VaultBrowser._showMenu(plugin, tree)
-- directly (NOT showBrowser, which would re-scan the whole vault and lose
-- the in-memory toggle state).
function VaultBrowser.buildItemTable(tree, plugin)
    local items = {}

    local function refresh()
        UIManager:nextTick(function()
            if plugin._vault_browser_menu then
                pcall(function() UIManager:close(plugin._vault_browser_menu) end)
                plugin._vault_browser_menu = nil
            end
            VaultBrowser._showMenu(plugin, tree)
        end)
    end

    -- Toolbar: collapse all
    table.insert(items, {
        text = _("⤿ Collapse all"),
        enabled_func = function() return true end,
        keep_menu_open = true,
        callback = function()
            VaultBrowser.collapseAll(tree)
            refresh()
        end,
    })
    -- Toolbar: expand all
    table.insert(items, {
        text = _("⤢ Expand all"),
        enabled_func = function() return true end,
        keep_menu_open = true,
        callback = function()
            VaultBrowser.expandAll(tree)
            refresh()
        end,
    })
    -- Toolbar: focus current
    table.insert(items, {
        text = _("◎ Focus current"),
        enabled_func = function() return true end,
        keep_menu_open = true,
        callback = function()
            local current_file = plugin.ui.document and plugin.ui.document.file
            if current_file then
                local chain = VaultBrowser.findChain(tree, current_file)
                if chain then
                    for _, n in ipairs(chain) do
                        n.expanded = true
                    end
                    refresh()
                else
                    UIManager:show(InfoMessage:new{ text = _("Current file not in vault."), timeout = 2 })
                end
            end
        end,
    })
    -- Separator
    table.insert(items, {
        text = _("───────────────────"),
        enabled_func = function() return false end,
    })

    -- Tree items
    local tree_items = VaultBrowser.flattenTree(tree)
    for _, ti in ipairs(tree_items) do
        if ti.type == "directory" then
            ti.enabled_func = function() return true end
            ti.keep_menu_open = true
            ti.callback = function()
                VaultBrowser.toggleNode(tree, ti.path)
                refresh()
            end
        else
            ti.enabled_func = function() return true end
            ti.callback = function()
                if plugin._vault_browser_menu then
                    UIManager:close(plugin._vault_browser_menu)
                    plugin._vault_browser_menu = nil
                end
                plugin.ui:switchDocument(ti.path)
            end
        end
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

    -- Validate vault root is an accessible directory before scanning
    local root_mode = lfs.attributes(vault_root, "mode")
    if root_mode ~= "directory" then
        UIManager:show(InfoMessage:new{
            text = _("Vault root is not a directory or is not accessible: ") .. tostring(vault_root),
            timeout = 5,
        })
        return
    end

    -- Close any existing browser menu so we don't stack menus
    if plugin._vault_browser_menu then
        pcall(function() UIManager:close(plugin._vault_browser_menu) end)
        plugin._vault_browser_menu = nil
    end

    -- Scan the vault (cycle-safe + is_dir fallback fixed in listDirSorted).
    -- pcall so a single bad file or permission error can't crash KOReader.
    local ok, tree = pcall(VaultBrowser.scanVault, vault_root)
    if not ok then
        UIManager:show(InfoMessage:new{
            text = _("Failed to scan vault: ") .. tostring(tree),
            timeout = 5,
        })
        return
    end

    -- Load and apply persisted expand-state
    local expand_state = plugin.settings:readSetting("tree_expanded") or {}
    local function applyState(node)
        if expand_state[node.path] == "expanded" then
            node.expanded = true
        elseif expand_state[node.path] == "collapsed" then
            node.expanded = false
        end
        if node.children then
            for _, child in ipairs(node.children) do
                applyState(child)
            end
        end
    end
    applyState(tree)

    local function saveState(node)
        if node.children and #node.children > 0 then
            expand_state[node.path] = node.expanded and "expanded" or "collapsed"
            for _, child in ipairs(node.children) do
                saveState(child)
            end
        end
    end

    local function persist()
        saveState(tree)
        plugin.settings:saveSetting("tree_expanded", expand_state)
        plugin:saveSettings()
    end

    VaultBrowser._showMenu(plugin, tree)
end

-- Build and show the vault browser menu from an already-built in-memory tree.
-- Used by refresh (after toggle) so toggles are preserved without re-scanning.
-- Used by showBrowser (initial) so menu construction is in one place.
function VaultBrowser._showMenu(plugin, tree)
    local menu = Menu:new{
        title = _("Vault Browser"),
        item_table = VaultBrowser.buildItemTable(tree, plugin),
        covers_fullscreen = true,
        is_borderless = true,
        width = nil,
        height = nil,
        close_callback = function()
            if plugin._vault_browser_menu == menu then
                plugin._vault_browser_menu = nil
            end
            UIManager:close(menu)
        end,
    }
    plugin._vault_browser_menu = menu
    UIManager:show(menu)
end

return VaultBrowser
