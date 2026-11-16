--!strict
-- The Settings pages, in sidebar order (screens/8). To add a page: write
-- screens/settings/<id>.luau returning `function(): Node` (build it from
-- screens/settings/parts.luau) and set `page` here; `on_key` takes the
-- page's shortcuts. A page without one shows empty.
local actions = require("@app/actions")

export type Page = {
	id: string,
	label: string,
	-- An icon of lib/icons.luau.
	icon: string,
	page: (() -> Node)?,
	-- Keys pressed in Settings while the page shows; true when taken.
	on_key: ((e: { key: string, ctrl: boolean, shift: boolean, alt: boolean }) -> boolean)?,
}

local pages: { Page } = {
	{ id = "general", label = "General", icon = "settings" },
	{ id = "captions", label = "Captions", icon = "captions" },
	{ id = "audio", label = "Audio", icon = "audio" },
	{
		id = "providers",
		label = "Providers",
		icon = "key",
		page = require("./providers"),
		on_key = function(e)
			if e.ctrl and string.lower(e.key) == "n" then
				actions.open_add_provider()
				return true
			end
			return false
		end,
	},
	{ id = "glossary", label = "Glossary", icon = "glossary" },
	{ id = "shortcuts", label = "Shortcuts", icon = "keyboard" },
	{ id = "storage", label = "Storage", icon = "storage" },
}

return pages
