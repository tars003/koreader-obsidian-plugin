# Glossary — obsidian-kindle

> Shared vocabulary for the obsidian-kindle project. Used in chat, code, and docs.
> Update protocol: when a new term is introduced, add it here with a `NEW:` marker, then drop the marker once the term has been reused.

- **addToMainMenu** — plugin method called when the main menu opens
- **back_stack** — the plugin's back-navigation history (a Lua table on the plugin instance)
- **debug log** — `/mnt/us/koreader/obsidian-debug.log`, written by the plugin
- **enabled_func** — menu-item callback that returns true/false to enable/disable a menu item
- **is_dir fallback** — the `pcall`/`lfs.dir` probe in listDirSorted that mis-classifies files when lfs.attributes fails
- **is_doc_only** — plugin flag; `false` means the plugin lives in the file manager and is also reachable from the reader
- **Kindle** — the hardware (Kindle Paperwhite 7th gen in this project)
- **KOReader** — the e-ink reader app on the Kindle
- **link handler** — the wrapper installed on ReaderLink by linkhandler.lua
- **listDirSorted** — helper inside vaultbrowser.lua; classifies one directory's entries
- **md2kindle** — the Python converter
- **NEW:** — marker for a term introduced in chat that has not yet been reused
- **obsidian.koplugin** — the Lua plugin
- **onGoToExternalLink** — ReaderLink method invoked for "external-looking" links (relative URLs included)
- **openFileFromLink** — older ReaderLink method, same purpose as onGoToExternalLink
- **registerScheme** — KOReader API call that hooks empty-scheme links
- **registerToMainMenu** — KOReader hook where a plugin announces its `addToMainMenu` method
- **scanVault** — function in vaultbrowser.lua; walks vault root, returns a tree
- **switchDocument** — KOReader API that opens a different file in the reader
- **vault browser** — the menu UI built by vaultbrowser.lua
- **vault root** — directory the vault browser scans; set via "Vault root" menu, stored in `vault_root` setting

## Terms introduced after v1 (pending reuse)

- **NEW: refresh** — close-current-menu + re-show-vault-browser, used in the vault browser toolbar callbacks
- **NEW: vault tree** — the nested structure returned by scanVault (folders + .html files)
