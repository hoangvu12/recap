--!strict
-- The Notes window (screens/2 "Notes live"): header with the tabs, the tab's
-- content, the footer action bar.
local state = require("@app/state")
local host = require("@app/host")
local processing = require("@lib/processing")
local actions = require("@app/actions")
local meeting_actions = require("@lib/meeting_actions")
local ActionPanel = require("@components/action_panel")
local ConfirmDelete = require("@components/confirm_delete")
local Hud = require("@components/hud")
local Header = require("@self/header")
local Footer = require("@self/footer")
local Transcript = require("@self/transcript")
local Summary = require("@self/summary")

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

return function(): Node
	-- Done processing (DESIGN §2): the summary written while the card showed
	-- this meeting opens the Summary tab.
	ui.effect(function()
		local s = host.summary:get()
		local p = host.processing:peek()
		if s.id ~= "" and p.id == s.id and #p.steps > 0 then
			state.notes_tab:set("summary")
		end
	end)
	return ui.column {
		id = "notes-window",
		w_full = true,
		h_full = true,
		bg = "$rc.bg.window",
		color = "$rc.text.primary",
		font = "$rc.font.ui",
		on_key = function(e)
			if e.ctrl and (e.key == "k" or e.key == "K") then
				actions.toggle_actions("notes")
				return true
			end
			local action = meeting_actions.for_key(e)
			if action and actions.action_target("notes") then
				actions.run_action("notes", action)
				return true
			end
			if e.key == "Enter" and state.notes_tab:peek() ~= "notes" then
				actions.primary()
				return true
			end
			-- Processing goes on with the window closed (Hide Esc).
			if e.key == "Escape" and processing.working(host.processing:peek()) then
				actions.hide_notes()
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
		Hud(),
		ActionPanel("notes"),
		ConfirmDelete("notes"),
	}
end
