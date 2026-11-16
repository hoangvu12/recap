--!strict
-- The inline rename of a speaker (screens/5): a field where the chip was,
-- focused at once, and under it the known people matching what is typed,
-- then `Add “…”`. Up and Down pick, Enter takes the one picked, a click
-- takes one, Escape leaves it as it was. The list opens from the field
-- (150 ms, motion_spec §2.9) and keys stay in the field.
local state = require("@app/state")
local host = require("@app/host")
local actions = require("@app/actions")
local speakers = require("@lib/speakers")
local motion = require("@lib/motion")
local Icon = require("./icon")

local function Option(i: number, s: speakers.Suggestion): Node
	local picked = function(): boolean
		return state.rename_pick:get() == i
	end
	return ui.button {
		id = `rename-option-{i}`,
		w_full = true,
		h = 32,
		px = 8,
		gap = 10,
		items = "center",
		justify = "start",
		rounded = 6,
		bg = function()
			return if picked() then "$rc.fill.selected" else "$rc.transparent"
		end,
		hover = { bg = "$rc.fill.hover" },
		on_click = function()
			actions.commit_rename(s.name)
		end,
		Icon { name = if s.add then "plus" else "speakers", size = 14 },
		ui.text { s.label, literal = true, text_size = 13, color = "$rc.text.primary", truncate = true },
	}
end

return function(index: number): Node
	-- Who is known, read once a rename starts.
	local people = ui.computed(function(): { string }
		return if state.renaming:get().index == index then host.people() else {}
	end)
	local options = ui.computed(function(): { speakers.Suggestion }
		return speakers.suggestions(people:get(), state.rename_text:get(), nil)
	end)
	local numbered = ui.computed(function(): { { i: number, s: speakers.Suggestion } }
		local out = {}
		for i, s in options:get() do
			table.insert(out, { i = i, s = s })
		end
		return out
	end)
	-- Typing starts the pick again at the top.
	ui.effect(function()
		state.rename_text:get()
		state.rename_pick:set(1)
	end)
	local function on_key(e: any): boolean
		local list = options:peek()
		local key = e.key
		if key == "ArrowDown" then
			state.rename_pick:set(math.min(state.rename_pick:peek() + 1, math.max(#list, 1)))
			return true
		elseif key == "ArrowUp" then
			state.rename_pick:set(math.max(state.rename_pick:peek() - 1, 1))
			return true
		elseif key == "Enter" then
			local pick = list[state.rename_pick:peek()]
			if pick then
				actions.commit_rename(pick.name)
			else
				actions.cancel_rename()
			end
			return true
		elseif key == "Escape" then
			actions.cancel_rename()
			return true
		end
		return false
	end
	-- The list is open while this row renames; closed from outside (a click
	-- away), the rename is left.
	local open = ui.state(`rename_open.{index}`, false)
	ui.effect(function()
		open:set(state.renaming:get().index == index)
	end)
	ui.effect(function()
		if not open:get() and state.renaming:peek().index == index then
			actions.cancel_rename()
		end
	end)
	return ui.popover {
		open = open,
		at_caret = true,
		label = "Who is this?",
		anchor = ui.input {
			id = "rename-input",
			value = state.rename_text,
			label = "Speaker name",
			placeholder = "Who is this?",
			placeholder_color = "$rc.text.tertiary",
			w = 200,
			h = 26,
			px = 8,
			text_size = 13,
			rounded = 6,
			bg = "$rc.bg.raised",
			border = 1,
			border_color = "$rc.stroke.field",
			on_mount = function(el)
				el:focus()
			end,
			on_key = on_key,
		},
		build = function(): Node
			-- Opens with a 150 ms fade and scale .98 → 1 (motion_spec §2.9).
			return motion.Enter("column", {
				id = "rename-options",
				w = 240,
				p = 4,
				gap = 2,
				ui.each(numbered, function(o: { i: number, s: speakers.Suggestion })
					return Option(o.i, o.s)
				end, {
					-- By place and label: an option is rebuilt when either moves.
					key = function(o: { i: number, s: speakers.Suggestion }): string
						return `{o.i}|{o.s.label}`
					end,
				}),
			}, motion.POP)
		end,
	}
end
