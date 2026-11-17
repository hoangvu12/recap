--!strict
-- Settings (screens/8, 8b): the title strip, the sidebar (search and the
-- pages of pages.luau), and the chosen page. Pages are added in
-- pages.luau; this shell needs no change for them.
local state = require("@app/state")
local actions = require("@app/actions")
local Icon = require("@components/icon")
local Tabs = require("@components/tabs")
local pages = require("@self/pages")

local function TitleStrip(): Node
	return ui.row {
		id = "settings-titlebar",
		h = 40,
		w_full = true,
		shrink = 0,
		px = 12,
		gap = 14,
		items = "center",
		window_drag = true,
		Icon { name = "chevron-left", color = "$rc.text.secondary" },
		Icon { name = "chevron-right", color = "$rc.text.quaternary" },
		ui.text { "Settings", id = "settings-title", text_size = 13, color = "$rc.text.secondary" },
	}
end

local function matches(page: pages.Page, query: string): boolean
	return query == "" or string.find(string.lower(page.label), string.lower(query), 1, true) ~= nil
end

local function NavItem(page: pages.Page): Node
	local selected = function()
		return state.settings_page:get() == page.id
	end
	return ui.show(
		function()
			return matches(page, state.settings_query:get())
		end,
		Tabs.Tab {
			id = `nav-{page.id}`,
			selected = selected,
			h = 32,
			w_full = true,
			shrink = 0,
			p = 0,
			rounded = 6,
			bg = "$rc.transparent",
			hover = { bg = "$rc.fill.hover" },
			on_select = function()
				actions.show_settings_page(page.id)
			end,
			ui.row {
				w_full = true,
				h_full = true,
				px = 10,
				gap = 10,
				items = "center",
				rounded = 6,
				bg = function()
					return if selected() then "$rc.fill.selected" else "$rc.transparent"
				end,
				Icon { name = page.icon, color = "$rc.text.secondary" },
				ui.text { page.label, text_size = 13, color = "$rc.text.primary" },
			},
		}
	)
end

local function Sidebar(): Node
	local items = {}
	for _, page in pages do
		table.insert(items, NavItem(page))
	end
	return ui.column {
		id = "settings-sidebar",
		w = 230,
		shrink = 0,
		h_full = true,
		p = 8,
		gap = 2,
		ui.row {
			h = 32,
			w_full = true,
			shrink = 0,
			px = 10,
			gap = 8,
			mb = 6,
			items = "center",
			rounded = 8,
			bg = "$rc.fill.input",
			Icon { name = "search", color = "$rc.text.tertiary" },
			ui.input {
				id = "settings-search",
				label = "Search settings",
				value = state.settings_query,
				placeholder = "Search settings",
				placeholder_color = "$rc.text.tertiary",
				w = 0,
				grow = 1,
				h = 32,
				text_size = 13,
				bg = "$rc.transparent",
				border = 0,
				p = 0,
			},
		},
		Tabs.List { label = "Settings", vertical = true, w_full = true, gap = 2, items },
	}
end

local function Content(): Node
	local cases = {}
	for _, page in pages do
		local build = page.page
		cases[page.id] = if build then build() else ui.column { id = `page-{page.id}`, w_full = true }
	end
	return ui.scroll {
		id = "settings-content",
		grow = 1,
		basis = 0,
		h_full = true,
		ui.column {
			w_full = true,
			px = 16,
			pt = 12,
			pb = 24,
			ui.switch(state.settings_page, cases, ui.column { w_full = true }),
		},
	}
end

local function page_keys(e): boolean
	for _, page in pages do
		if page.id == state.settings_page:peek() and page.on_key then
			return page.on_key(e)
		end
	end
	return false
end

return function(): Node
	return ui.column {
		id = "settings-window",
		w_full = true,
		h_full = true,
		bg = "$rc.bg.canvas",
		color = "$rc.text.primary",
		font = "$rc.font.ui",
		on_key = page_keys,
		TitleStrip(),
		ui.div { h = 1, w_full = true, shrink = 0, bg = "$rc.stroke.hairline" },
		ui.row {
			w_full = true,
			grow = 1,
			basis = 0,
			items = "stretch",
			overflow = "clip",
			Sidebar(),
			ui.div { w = 1, shrink = 0, bg = "$rc.stroke.hairline" },
			Content(),
		},
	}
end
