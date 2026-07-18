-- plugin/obsidian.koplugin/main.lua
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local UIManager = require("ui/uimanager")
local InfoMessage = require("ui/widget/infomessage")
local LuaSettings = require("luasettings")
local DataStorage = require("datastorage")
local LinkHandler = require("linkhandler")
local T = require("ffi/util").template
local _ = require("gettext")

-- Module-level shared state.
-- KOReader creates a SEPARATE plugin instance for each UI context
-- (file manager, each reader, etc.), so any state that must survive
-- across instances must live in _G (the global table), not in a local.
-- A local at the top of this file is NOT shared — KOReader uses dofile
-- (not require) to load plugins, so each instance gets its own copy.
-- _G is the one table that every Lua chunk shares.
-- Confirmed by diagnostic log: 5 distinct plugin instances in one session.
_G._obsidian_shared = _G._obsidian_shared or {
    back_stack = {},
}

local ObsidianPlugin = WidgetContainer:extend{
    name = "obsidian",
    is_doc_only = false,
}

function ObsidianPlugin:init()
    self._shared = _G._obsidian_shared
    self:loadSettings()
    self.ui.menu:registerToMainMenu(self)
    -- link handler: only active when a document is open
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

function ObsidianPlugin:loadSettings()
    self.settings = LuaSettings:open(
        DataStorage:getSettingsDir() .. "/obsidian.lua"
    )
end

function ObsidianPlugin:saveSettings()
    self.settings:flush()
end

function ObsidianPlugin:addToMainMenu(menu_items)
    -- Install link handler in reader context (init only runs in FM since is_doc_only=false)
    if self.ui.document and not self._link_handler_installed then
        self:initLinkHandler()
        self._link_handler_installed = true
    end
    self:logLinkEvent("addToMainMenu", "self=" .. tostring(self)
        .. " | shared=" .. tostring(self._shared)
        .. " | back_stack.len=" .. tostring(#self._shared.back_stack)
        .. " | ui.document.file=" .. tostring(self.ui.document and self.ui.document.file))
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
                enabled_func = function() return #self._shared.back_stack > 0 end,
                separator = true,
                callback = function() self:goBack() end,
            },
            {
                text = _("Clear navigation history"),
                enabled_func = function() return #self._shared.back_stack > 0 end,
                callback = function() self:clearBackStack() end,
            },
            {
                text = T(_("Vault root: %1"), vault_root_text),
                separator = true,
                keep_menu_open = true,
                callback = function() self:promptVaultRoot() end,
            },
            {
                text = _("Debug: write to obsidian-debug.log"),
                keep_menu_open = true,
                callback = function() self:dumpDebug() end,
            },
        },
    }
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

function ObsidianPlugin:openVaultBrowser()
    local VaultBrowser = require("vaultbrowser")
    VaultBrowser.showBrowser(self)
end

function ObsidianPlugin:goBack()
    if #self._shared.back_stack > 0 then
        local previous = table.remove(self._shared.back_stack)
        self.ui:switchDocument(previous)
    elseif self.ui.document then
        -- Back-stack empty and a document is open — close it to return to file manager
        self.ui:onCloseDocument()
    end
end

function ObsidianPlugin:clearBackStack()
    -- Clear in place so the shared table reference stays the same
    for i = #self._shared.back_stack, 1, -1 do
        self._shared.back_stack[i] = nil
    end
end

function ObsidianPlugin:initLinkHandler()
    if self._link_handler_installed then return end
    LinkHandler.install(self)
    self._link_handler_installed = true
end

function ObsidianPlugin:onFlushSettings()
    self:saveSettings()
end

function ObsidianPlugin:onCloseDocument()
    -- Do NOT clear back_stack on document close — that fires on every
    -- switchDocument navigation, wiping our just-pushed entry. The user
    -- clears it explicitly via the menu.
    self._link_handler_installed = false
end

function ObsidianPlugin:onOpenDocument()
    -- Install link handler as soon as a document opens (not just when menu is opened)
    self:initLinkHandler()
end

function ObsidianPlugin:onReaderReady()
    -- Some KOReader versions fire onReaderReady instead of onOpenDocument
    self:initLinkHandler()
end

function ObsidianPlugin:dumpDebug()
    -- Write a diagnostic dump to /mnt/us/koreader/obsidian-debug.log
    local f = io.open("/mnt/us/koreader/obsidian-debug.log", "a")
    if f then
        f:write("\n=== debug " .. os.date() .. " ===\n")
        f:write("vault_root: " .. tostring(self.settings:readSetting("vault_root")) .. "\n")
        f:write("ui.document.file: " .. tostring(self.ui.document and self.ui.document.file) .. "\n")
        f:write("shared: " .. tostring(self._shared) .. "\n")
        f:write("back_stack length: " .. tostring(#self._shared.back_stack) .. "\n")
        f:write("back_stack contents:\n")
        for i, v in ipairs(self._shared.back_stack) do
            f:write("  " .. i .. ": " .. tostring(v) .. "\n")
        end
        f:write("link_handler_installed: " .. tostring(self._link_handler_installed) .. "\n")
        f:write("ui.document: " .. tostring(self.ui.document) .. "\n")
        f:write("ui.link: " .. tostring(self.ui and self.ui.link) .. "\n")
        f:close()
    end
end

function ObsidianPlugin:logLinkEvent(event, details)
    local f = io.open("/mnt/us/koreader/obsidian-debug.log", "a")
    if f then
        f:write("[" .. os.date("%H:%M:%S") .. "] " .. event .. " " .. tostring(details or "") .. "\n")
        f:close()
    end
end

return ObsidianPlugin
