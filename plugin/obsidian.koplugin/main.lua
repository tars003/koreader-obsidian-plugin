-- plugin/obsidian.koplugin/main.lua
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local UIManager = require("ui/uimanager")
local InfoMessage = require("ui/widget/infomessage")
local LuaSettings = require("luasettings")
local DataStorage = require("datastorage")
local LinkHandler = require("linkhandler")
local T = require("ffi/util").template
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
    if #self.back_stack > 0 then
        local previous = table.remove(self.back_stack)
        self.ui:switchDocument(previous)
    end
end

function ObsidianPlugin:clearBackStack()
    self.back_stack = {}
end

function ObsidianPlugin:initLinkHandler()
    LinkHandler.install(self)
end

function ObsidianPlugin:onFlushSettings()
    self:saveSettings()
end

function ObsidianPlugin:onCloseDocument()
    self:clearBackStack()
end

return ObsidianPlugin
