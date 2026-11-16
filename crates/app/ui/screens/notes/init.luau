--!strict
-- The Notes window (screens/2 "Notes live"): header with the tabs, the tab's
-- content, the footer action bar.
local state = require("@app/state")
local actions = require("@app/actions")
local Header = require("@self/header")
local Footer = require("@self/footer")
local Transcript = require("@self/transcript")

local function MyNotes(): Node
	return ui.column {
		id = "notes-tab",
		w_full = true,
		grow = 1,
		p = 16,
		ui.textarea {
			id = "my-notes",
			label = "My notes",
			value = state.my_notes,
			placeholder = "Type your notes",
			placeholder_color = "$rc.text.tertiary",
			wrap = true,
			w_full = true,
			grow = 1,
			text_size = 13,
			bg = "$rc.transparent",
			border = 0,
			p = 0,
		},
	}
end

-- Filled by the processing and summary tickets (#9, #11).
local function Summary(): Node
	return ui.column { id = "summary-tab", w_full = true, grow = 1, p = 16 }
end

return function(): Node
	return ui.column {
		id = "notes-window",
		w_full = true,
		h_full = true,
		bg = "$rc.bg.window",
		color = "$rc.text.primary",
		font = "$rc.font.ui",
		on_key = function(e)
			if e.ctrl and (e.key == "k" or e.key == "K") then
				actions.toggle_actions()
				return true
			end
			if e.key == "Enter" and state.notes_tab:peek() ~= "notes" then
				actions.primary()
				return true
			end
			return false
		end,
		Header(),
		ui.switch(state.notes_tab, {
			notes = MyNotes(),
			transcript = Transcript(),
			summary = Summary(),
		}),
		Footer(),
	}
end
