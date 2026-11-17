--!strict
-- Home (screens/7 "Home · meetings"): the search header, the meetings list
-- beside the selected meeting's detail, the footer action bar.
-- ↑/↓ select, Enter (or a double click) opens the meeting in the Notes
-- window, read mode.
local host = require("@app/host")
local state = require("@app/state")
local actions = require("@app/actions")
local meetings = require("@lib/meetings")
local Icon = require("@components/icon")
local Button = require("@components/button")
local IconButton = require("@components/icon_button")
local meeting_actions = require("@lib/meeting_actions")
local ActionPanel = require("@components/action_panel")
local ConfirmDelete = require("@components/confirm_delete")
local Hud = require("@components/hud")
local Announce = require("@components/announce")
local List = require("@self/list")
local Detail = require("@self/detail")

-- Home's search, once mounted: it gets the keyboard back when the action
-- panel closes.
local search_field: any = nil

local function Header(): Node
	return ui.row {
		id = "home-header",
		h = 64,
		w_full = true,
		shrink = 0,
		px = 16,
		gap = 12,
		items = "center",
		window_drag = true,
		ui.row { w = 20, h = 20, shrink = 0, items = "center", Icon { name = "search" } },
		ui.input {
			id = "home-search",
			label = "Search meetings",
			value = state.home_query,
			placeholder = "Search meetings, people, words…",
			placeholder_color = "$rc.text.tertiary",
			grow = 1,
			basis = 0,
			text_size = 18,
			bg = "$rc.transparent",
			border = 0,
			p = 0,
			-- No ring: Raycast's search shows its caret only. (Windows'
			-- focus ring stayed drawn round it beside the open action
			-- panel, whose search has the keyboard.)
			focus = { outline = 0 },
			on_mount = function(el)
				search_field = el
			end,
		},
		IconButton {
			id = "home-settings",
			icon = "settings",
			label = "Settings",
			keys = { "Ctrl", "," },
			on_click = actions.open_settings,
			tip_side = "bottom",
		},
	}
end

local function Footer(selected: ReadOnlyState<string>): Node
	return ui.column {
		id = "home-footer",
		w_full = true,
		shrink = 0,
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
		ui.row {
			h = 42,
			pl = 16,
			pr = 12,
			gap = 8,
			items = "center",
			Icon { name = "captions" },
			ui.text { "Meetings", text_size = 13, color = "$rc.text.secondary" },
			ui.div { grow = 1 },
			-- Both act on the selected meeting: with none (an empty Home)
			-- they don't show, rather than show and do nothing.
			ui.show(
				function()
					return selected:get() ~= ""
				end,
				ui.row {
					items = "center",
					gap = 4,
					Button {
						id = "open-meeting",
						label = "Open Meeting",
						keys = { "↵" },
						on_click = actions.open_selected,
					},
					ui.div { w = 1, h = 16, mx = 4, bg = "$rc.stroke.hairline" },
					-- Ctrl K opens it from anywhere in Home; it hangs above this
					-- button.
					ActionPanel {
						where = "home",
						anchor = Button {
							id = "home-actions",
							label = "Actions",
							color = "$rc.text.secondary",
							keys = { "Ctrl", "K" },
							on_click = function()
								actions.toggle_actions("home")
							end,
						},
					},
				}
			),
		},
	}
end

return function(): Node
	local shown = ui.computed(function()
		local query = state.home_query:get()
		return meetings.visible(host.meetings:get(), query, host.search(query))
	end)
	local selected = ui.computed(function(): string
		return meetings.pick(shown:get(), state.home_selected:get())
	end)
	-- The selection follows what shows: the first, when it was filtered out.
	ui.effect(function()
		local id = selected:get()
		if state.home_selected:peek() ~= id then
			state.home_selected:set(id)
		end
	end)
	-- The action panel closed: the keyboard is back in the search.
	local panel_was_open = false
	ui.effect(function()
		local open = state.actions_open:get() == "home"
		if panel_was_open and not open and search_field then
			search_field:focus()
		end
		panel_was_open = open
	end)
	return ui.column {
		id = "home-window",
		w_full = true,
		h_full = true,
		bg = "$rc.bg.window",
		color = "$rc.text.primary",
		font = "$rc.font.ui",
		on_key = function(e)
			if e.ctrl and (e.key == "k" or e.key == "K") then
				actions.toggle_actions("home")
				return true
			end
			local action = meeting_actions.for_key(e)
			if action and actions.action_target("home") then
				actions.run_action("home", action)
				return true
			end
			if e.ctrl and e.key == "," then
				actions.open_settings()
				return true
			end
			if e.key == "ArrowDown" or e.key == "ArrowUp" then
				actions.move_selection(if e.key == "ArrowDown" then 1 else -1)
				return true
			end
			if e.key == "Enter" then
				actions.open_selected()
				return true
			end
			return false
		end,
		Header(),
		ui.div { h = 1, w_full = true, shrink = 0, bg = "$rc.stroke.hairline" },
		ui.row {
			w_full = true,
			grow = 1,
			basis = 0,
			items = "stretch",
			overflow = "clip",
			List { shown = shown, selected = selected },
			ui.div { w = 1, shrink = 0, bg = "$rc.stroke.hairline" },
			Detail { selected = selected },
		},
		Footer(selected),
		Hud(),
		Announce {
			id = "home-announce",
			text = function()
				return state.hud:get().text
			end,
		},
		ConfirmDelete("home"),
	}
end
