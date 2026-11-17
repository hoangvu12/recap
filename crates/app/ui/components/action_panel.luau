--!strict
-- The action panel (Ctrl K, screens/9; raycast_design "Action panel"): a
-- 350 wide popover (components/popover) 8 above the footer's "Actions"
-- button, at its right edge, with the meeting's title, its actions with
-- their keys, and "Search for actions…" at the bottom. The search keeps the
-- keyboard: type to filter, ↑/↓ move the highlighted action (the search's
-- active descendant), Enter runs it, Esc, Ctrl K or a click outside
-- closes; an action's keys run it at once. It opens instantly, as
-- Raycast's does: a keyboard surface opened many times a day.
--
-- Ids: `action-panel`, `action-panel-title`, `action-<id>` (role option),
-- `action-<id>-label`, `action-search`.
local state = require("@app/state")
local actions = require("@app/actions")
local meeting_actions = require("@lib/meeting_actions")
local Icon = require("@components/icon")
local Keycap = require("@components/keycap")
local Popover = require("@components/popover")

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
			-- The search keeps the keyboard; arrows move the highlight.
			focusable = false,
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

-- The panel's content: the title, the actions, the search.
local function Body(where: state.Where, keys: (any) -> boolean): Node
	local items = {}
	for _, action in meeting_actions.ALL do
		table.insert(items, Item(where, action))
	end
	return ui.column {
		id = "action-panel",
		w_full = true,
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
			active_descendant = function(): string?
				local id = meeting_actions.pick(state.actions_query:get(), state.actions_selected:get())
				return if id ~= "" then `action-{id}` else nil
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
end

export type Props = { where: state.Where, anchor: Node }

-- Written where its trigger is: `anchor` is the footer's Actions button.
return function(props: Props): Node
	local where = props.where
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
	return Popover {
		open = function()
			return state.actions_open:get() == where
		end,
		on_close = actions.close_actions,
		label = "Actions",
		anchor = props.anchor,
		side = "top",
		align = "end",
		offset = 8,
		w = 350,
		build = function(): Node
			return Body(where, keys)
		end,
	}
end
