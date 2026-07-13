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
    UIManager:show(InfoMessage:new{ text = _("Vault root setting will prompt for path (Task 2).") })
end

function ObsidianPlugin:openVaultBrowser()
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
