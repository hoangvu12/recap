--!strict
-- What the user can do: buttons, keys and later the action panel call these,
-- so the same action behind each is one function.
local host = require("@app/host")
local state = require("@app/state")
local meetings = require("@lib/meetings")

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

-- Start new notes: a new meeting, live.
function M.start_notes()
	host.invoke("start_notes")
end

-- A stored meeting in the Notes window, read mode.
function M.open_meeting(id: string)
	if id ~= "" then
		host.invoke("open_meeting", id)
	end
end

-- The meetings Home shows now, newest first.
function M.home_shown()
	return meetings.visible(host.meetings:peek(), state.home_query:peek())
end

function M.select_meeting(id: string)
	state.home_selected:set(id)
end

-- Move Home's selection `delta` rows (the arrows).
function M.move_selection(delta: number)
	local shown = M.home_shown()
	local at = meetings.pick(shown, state.home_selected:peek())
	state.home_selected:set(meetings.step(shown, at, delta))
end

-- Open Home's selected meeting (Enter, Open Meeting).
function M.open_selected()
	M.open_meeting(meetings.pick(M.home_shown(), state.home_selected:peek()))
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
