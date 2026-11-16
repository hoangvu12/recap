--!strict
-- Delete meeting asks first (a Raycast alert): the meeting's title, Delete ↵
-- (red, focused) and Cancel Esc. Ids: `confirm-delete`,
-- `confirm-delete-title`, `confirm-delete-yes`, `confirm-delete-no`.
local state = require("@app/state")
local actions = require("@app/actions")
local Icon = require("@components/icon")
local Keycap = require("@components/keycap")

local function Button(props: { id: string, label: string, color: string, key: string, on_click: () -> () }): Node
	return ui.button {
		id = props.id,
		h = 32,
		px = 10,
		gap = 8,
		grow = 1,
		basis = 0,
		items = "center",
		justify = "center",
		rounded = 6,
		bg = "$rc.fill.input",
		hover = { bg = "$rc.fill.hover" },
		on_click = props.on_click,
		ui.text { props.label, text_size = 13, weight = 500, color = props.color },
		Keycap(props.key),
	}
end

return function(where: state.Where): Node
	return ui.modal {
		open = function()
			local c = state.confirm_delete:get()
			return c.id ~= "" and c.where == where
		end,
		on_close = actions.cancel_delete,
		label = "Delete meeting",
		w = 320,
		p = 0,
		gap = 0,
		rounded = 12,
		bg = "$rc.bg.raised",
		border = 1,
		border_color = "$rc.border.raised",
		build = function(): Node
			return ui.column {
				id = "confirm-delete",
				w_full = true,
				p = 16,
				gap = 12,
				items = "center",
				on_key = function(e)
					if e.key == "Enter" then
						actions.confirm_delete()
						return true
					end
					if e.key == "Escape" then
						actions.cancel_delete()
						return true
					end
					return false
				end,
				Icon { name = "trash", size = 24, color = "$rc.red" },
				ui.text { "Delete meeting?", text_size = 13, weight = 600, color = "$rc.text.primary" },
				ui.text {
					function()
						return state.confirm_delete:get().title
					end,
					id = "confirm-delete-title",
					literal = true,
					w_full = true,
					align = "center",
					text_size = 12,
					color = "$rc.text.secondary",
				},
				ui.row {
					w_full = true,
					gap = 8,
					pt = 4,
					Button { id = "confirm-delete-yes", label = "Delete", color = "$rc.red", key = "↵", on_click = actions.confirm_delete },
					Button { id = "confirm-delete-no", label = "Cancel", color = "$rc.text.primary", key = "Esc", on_click = actions.cancel_delete },
				},
			}
		end,
	}
end
