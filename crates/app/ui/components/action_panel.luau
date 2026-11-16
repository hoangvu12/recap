--!strict
-- The action panel (Ctrl K, screens/9; raycast_design "Action panel"): a
-- 350 wide popover 8 above the footer at the window's right, with the
-- meeting's title, its actions with their keys, and "Search for actions…"
-- at the bottom. Keyboard only: type to filter, ↑/↓, Enter runs, Esc or
-- Ctrl K closes; an action's keys run it at once. It opens instantly, as
-- Raycast's does: a keyboard surface opened many times a day.
--
-- Ids: `action-panel`, `action-panel-title`, `action-<id>` (role option),
-- `action-<id>-label`, `action-search`.
local state = require("@app/state")
local actions = require("@app/actions")
local meeting_actions = require("@lib/meeting_actions")
local Icon = require("@components/icon")
local Keycap = require("@components/keycap")

-- How far the panel sits above the window's bottom: the footer (42) + 8.
local ABOVE = 50

local function Item(where: state.Where, action: meeting_actions.Action): Node
	local selected = function()
		return meeting_actions.pick(state.actions_query:get(), state.actions_selected:get()) == action.id
	end
	local color = if action.danger then "$rc.red" else "$rc.text.primary"
	local caps = {}
	for _, key in action.keys do
		table.insert(caps, Keycap(key))
	end
	return ui.show(
		function()
			for _, shown in meeting_actions.shown(state.actions_query:get()) do
				if shown.id == action.id then
					return true
				end
			end
			return false
		end,
		ui.button {
			id = `action-{action.id}`,
			role = "option",
			selected = selected,
			label = action.label,
			h = 32,
			w_full = true,
			shrink = 0,
			p = 0,
			rounded = 6,
			bg = "$rc.transparent",
			hover = { bg = "$rc.fill.hover" },
			on_click = function()
				actions.run_action(where, action.id)
			end,
			ui.row {
				w_full = true,
				h_full = true,
				px = 8,
				gap = 10,
				items = "center",
				rounded = 6,
				bg = function()
					return if selected() then "$rc.fill.selected" else "$rc.transparent"
				end,
				Icon { name = action.icon, color = color },
				ui.text {
					action.label,
					id = `action-{action.id}-label`,
					grow = 1,
					basis = 0,
					truncate = true,
					text_size = 13,
					color = color,
				},
				ui.row { gap = 2, shrink = 0, caps },
			},
		}
	)
end

return function(where: state.Where): Node
	local function keys(e): boolean
		if e.key == "Escape" or (e.ctrl and (e.key == "k" or e.key == "K")) then
			actions.close_actions()
			return true
		end
		if e.key == "ArrowDown" or e.key == "ArrowUp" then
			local q = state.actions_query:peek()
			state.actions_selected:set(meeting_actions.step(q, state.actions_selected:peek(), if e.key == "ArrowDown" then 1 else -1))
			return true
		end
		if e.key == "Enter" then
			local id = meeting_actions.pick(state.actions_query:peek(), state.actions_selected:peek())
			if id ~= "" then
				actions.run_action(where, id)
			end
			return true
		end
		local id = meeting_actions.for_key(e)
		if id then
			actions.run_action(where, id)
			return true
		end
		return false
	end
	local items = {}
	for _, action in meeting_actions.ALL do
		table.insert(items, Item(where, action))
	end
	return ui.show(
		function()
			return state.actions_open:get() == where
		end,
		ui.column {
			id = "action-panel",
			role = "dialog",
			label = "Actions",
			absolute = true,
			right = 8,
			bottom = ABOVE,
			w = 350,
			rounded = 8,
			bg = "$rc.bg.raised",
			border = 1,
			border_color = "$rc.border.raised",
			shadow = { { x = 0, y = 8, blur = 10, spread = -6, color = "#0000001A" }, { x = 0, y = 20, blur = 25, spread = -5, color = "#00000033" } },
			overflow = "clip",
			on_key = keys,
			ui.row {
				h = 32,
				w_full = true,
				px = 16,
				items = "end",
				pb = 4,
				ui.text {
					function()
						-- Read again each time the panel opens.
						state.actions_open:get()
						local target = actions.action_target(where)
						return if target then target.title else ""
					end,
					id = "action-panel-title",
					literal = true,
					truncate = true,
					text_size = 11,
					weight = 500,
					color = "$rc.text.secondary",
				},
			},
			ui.column { w_full = true, px = 8, pb = 8, items },
			ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
			ui.input {
				id = "action-search",
				label = "Search for actions",
				value = state.actions_query,
				on_key = keys,
				on_mount = function(el)
					el:focus()
				end,
				placeholder = "Search for actions…",
				placeholder_color = "$rc.text.tertiary",
				w_full = true,
				h = 40,
				px = 16,
				text_size = 13,
				bg = "$rc.transparent",
				border = 0,
			},
		}
	)
end
