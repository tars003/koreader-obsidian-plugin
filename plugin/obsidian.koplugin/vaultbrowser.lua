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
            local attr = lfs.attributes(full)
            -- lfs.attributes can return nil on FAT32; fall back to extension check
            local mode = (attr and attr.mode) or (name:match("%.html?$") and "file")
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
            else
                -- file (or fallback-tagged) — only include .html/.htm
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

local Menu = require("ui/widget/menu")
local UIManager = require("ui/uimanager")
local InfoMessage = require("ui/widget/infomessage")
local _ = require("gettext")

-- Build the flat item_table from the tree, with toolbar items
function VaultBrowser.buildItemTable(tree)
    local items = {
        {
            text = _("⤿ Collapse all"),
            action = "collapse",
            keep_menu_open = true,
        },
        {
            text = _("⤢ Expand all"),
            action = "expand",
            keep_menu_open = true,
        },
        {
            text = _("◎ Focus current"),
            action = "focus",
            keep_menu_open = true,
        },
        {
            text = _("───────────────────"),
            keep_menu_open = true,
            enabled_func = function() return false end,
        },
    }
    local tree_items = VaultBrowser.flattenTree(tree)
    for _, ti in ipairs(tree_items) do
        if ti.type == "directory" then
            ti.keep_menu_open = true
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

    local tree = VaultBrowser.scanVault(vault_root)

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

    local menu

    local function rebuildMenu()
        menu:switchItemTable(nil, VaultBrowser.buildItemTable(tree), -1)
    end

    menu = Menu:new{
        title = _("Vault Browser"),
        item_table = VaultBrowser.buildItemTable(tree),
        covers_fullscreen = true,
        is_borderless = true,
        width = nil,
        height = nil,
        close_callback = function()
            UIManager:close(menu)
        end,
        callback = function(item)
            if item.action == "collapse" then
                VaultBrowser.collapseAll(tree)
                persist()
                rebuildMenu()
            elseif item.action == "expand" then
                VaultBrowser.expandAll(tree)
                persist()
                rebuildMenu()
            elseif item.action == "focus" then
                local current_file = plugin.ui.document and plugin.ui.document.file
                if current_file then
                    local chain = VaultBrowser.findChain(tree, current_file)
                    if chain then
                        for _, n in ipairs(chain) do
                            n.expanded = true
                        end
                        persist()
                        rebuildMenu()
                    else
                        UIManager:show(InfoMessage:new{ text = _("Current file not in vault."), timeout = 2 })
                    end
                end
            elseif item.type == "directory" then
                VaultBrowser.toggleNode(tree, item.path)
                persist()
                rebuildMenu()
            elseif item.type == "file" then
                UIManager:close(menu)
                plugin.ui:switchDocument(item.path)
            end
        end,
    }

    UIManager:show(menu)
end

return VaultBrowser
