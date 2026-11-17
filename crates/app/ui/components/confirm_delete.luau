--!strict
-- Delete meeting asks first (a Raycast alert): the meeting's title, Delete ↵
-- (red) and Cancel Esc. The keyboard starts on Cancel, the least
-- destructive button (APG alert dialog); Enter still deletes, as its
-- keycap says, and Space or a click on Cancel cancels. Ids:
-- `confirm-delete`, `confirm-delete-title`, `confirm-delete-yes`,
-- `confirm-delete-no`.
local state = require("@app/state")
local actions = require("@app/actions")
local Icon = require("@components/icon")
local Button = require("@components/button")
local Dialog = require("@components/dialog")

return function(where: state.Where): Node
	return Dialog {
		open = function()
			local c = state.confirm_delete:get()
			return c.id ~= "" and c.where == where
		end,
		on_close = actions.cancel_delete,
		label = "Delete meeting",
		-- Cancel takes the keyboard itself.
		autofocus = false,
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
				ui.text { "Delete meeting?", heading = 2, text_size = 13, weight = 600, color = "$rc.text.primary" },
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
					Button {
						id = "confirm-delete-yes",
						label = "Delete",
						color = "$rc.red",
						keys = { "↵" },
						look = "filled",
						on_click = actions.confirm_delete,
					},
					Button {
						id = "confirm-delete-no",
						label = "Cancel",
						keys = { "Esc" },
						look = "filled",
						autofocus = true,
						on_click = actions.cancel_delete,
					},
				},
			}
		end,
	}
end
