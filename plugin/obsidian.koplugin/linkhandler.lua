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
    -- Strip anchor (#…) and query (?) before resolving
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
