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

return VaultBrowser
