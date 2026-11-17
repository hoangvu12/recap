--!strict
-- The inline rename of a speaker (screens/5): a name field where the chip
-- was (components/name_field), focused at once, the known people matching
-- what is typed under it, then `Add “…”`. Up and Down pick, Enter takes
-- the one picked, a click takes one, Escape leaves it as it was. The list
-- opens from the field (150 ms, motion_spec §2.9) and keys stay in the
-- field.
local state = require("@app/state")
local host = require("@app/host")
local actions = require("@app/actions")
local NameField = require("@components/name_field")

return function(index: number): Node
	-- Who is known, read once a rename starts.
	local people = ui.computed(function(): { string }
		return if state.renaming:get().index == index then host.people() else {}
	end)
	return NameField {
		id = "rename-input",
		label = "Speaker name",
		text = state.rename_text,
		people = function()
			return people:get()
		end,
		placeholder = "Who is this?",
		option_prefix = "rename-option",
		list_id = "rename-options",
		box = { w = 200, h = 26, pl = 8, pr = 8, bg = "$rc.bg.raised" },
		min_w = 240,
		autofocus = true,
		always_open = true,
		on_choose = actions.commit_rename,
		on_escape = actions.cancel_rename,
		-- Closed from outside (a click away): the rename is left.
		on_dismiss = function()
			if state.renaming:peek().index == index then
				actions.cancel_rename()
			end
		end,
	}
end
