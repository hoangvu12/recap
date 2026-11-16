--!strict
-- What the user can do: buttons, keys and later the action panel call these,
-- so the same action behind each is one function.
local host = require("@app/host")
local state = require("@app/state")

local M = {}

-- Stop the meeting (Stop ↵).
function M.stop()
	local s = host.meeting:peek().state
	if s == "recording" or s == "paused" then
		host.invoke("stop")
	end
end

function M.toggle_pause()
	local s = host.meeting:peek().state
	if s == "recording" then
		host.invoke("pause")
	elseif s == "paused" then
		host.invoke("resume")
	end
end

-- The action panel (Ctrl K).
function M.toggle_actions()
	state.actions_open:set(not state.actions_open:peek())
end

function M.show_tab(tab: state.NotesTab)
	state.notes_tab:set(tab)
end

-- Back to the newest row of the live transcript.
function M.jump_to_live(list: Element?)
	local count = host.transcript:peek().count
	if list and count > 0 then
		(list :: any):scroll_to_row(count, { align = "end" })
	end
	state.seen_count:set(count)
end

return M
