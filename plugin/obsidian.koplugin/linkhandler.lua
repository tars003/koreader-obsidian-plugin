-- plugin/obsidian.koplugin/linkhandler.lua
local ffiUtil = require("ffi/util")
local lfs = require("libs/libkoreader-lfs")
local UIManager = require("ui/uimanager")
local InfoMessage = require("ui/widget/infomessage")
local _ = require("gettext")

local LinkHandler = {}

-- Resolve a relative link_url against current_file's directory.
-- Returns an absolute path to the target .html file, or nil.
function LinkHandler.resolveTarget(link_url, current_file)
    -- Strip anchor (#…) and query (?) before resolving
    local path = link_url:gsub("[#?].*$", "")
    if path == "" then
        return nil  -- same-file anchor
    end
    -- Resolve relative to current document's dir
    local doc_dir = ""
    if current_file then
        doc_dir = current_file:match("(.*/)") or ""
    end
    local resolved = ffiUtil.joinPath(doc_dir, path)
    -- Normalize to absolute real path
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

-- Install wrappers on ReaderLink. Called from onOpenDocument / onReaderReady.
function LinkHandler.install(plugin)
    local link = plugin.ui.link
    if not link then return end
    if not plugin.ui.document then return end

    local vault_root = plugin.settings:readSetting("vault_root") or ""

    -- Try registering the empty scheme (for newer KOReader that supports it)
    -- Use pcall so this is safe on older versions
    pcall(function() link:registerScheme("") end)

    -- WRAPPER 1: onGoToExternalLink (for newer KOReader with registerScheme)
    -- Only fire if it's a relative link to a .html file
    local orig_onGoToExternalLink = link.onGoToExternalLink
    function link:onGoToExternalLink(link_url)
        if not link_url:match("^%w[%w+%-.]*:") and vault_root ~= "" then
            local target = LinkHandler.resolveTarget(link_url, plugin.ui.document.file)
            if target then
                table.insert(plugin.back_stack, plugin.ui.document.file)
                plugin.ui:switchDocument(target)
                return true
            end
        end
        return orig_onGoToExternalLink(self, link_url)
    end

    -- WRAPPER 2: openFileFromLink (for older KOReader / fallback)
    -- Relative file links go through this method. Push to back stack before opening.
    local orig_openFileFromLink = link.openFileFromLink
    function link:openFileFromLink(link_url)
        if not link_url:match("^%w[%w+%-.]*:") and vault_root ~= "" then
            local target = LinkHandler.resolveTarget(link_url, plugin.ui.document.file)
            if target then
                -- Push to back stack so "Go back" works
                table.insert(plugin.back_stack, plugin.ui.document.file)
            end
        end
        return orig_openFileFromLink(self, link_url)
    end
end

return LinkHandler
