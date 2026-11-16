--!strict
-- What the user can do: buttons, keys and later the action panel call these,
-- so the same action behind each is one function.
local host = require("@app/host")
local state = require("@app/state")
local meetings = require("@lib/meetings")
local providers = require("@lib/providers")

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

-- Settings ----------------------------------------------------------------

-- Open Settings (Ctrl+, in Home), on `page` if given.
function M.open_settings(page: string?)
	if page then
		state.settings_page:set(page)
	end
	host.windows.settings:set(true)
end

function M.show_settings_page(page: string)
	state.settings_page:set(page)
	state.hotkey_recording:set("")
end

-- Change settings: `patch` holds the fields that change
-- ({ partial_text = false }); the host saves them.
function M.set_settings(patch: { [string]: any })
	host.invoke("set_settings", patch)
end

-- Add the glossary's draft term (Enter, + Add).
function M.add_term()
	local term = state.glossary_draft:peek()
	if string.match(term, "%S") then
		host.invoke("add_term", term)
		state.glossary_draft:set("")
	end
end

function M.remove_term(term: string)
	host.invoke("remove_term", term)
end

-- Record the next keys pressed as hotkey `id`; again stops.
function M.record_hotkey(id: string)
	state.hotkey_recording:set(if state.hotkey_recording:peek() == id then "" else id)
end

-- The keys a hotkey being recorded takes from `e`: true when taken.
-- Esc stops, Backspace clears it, modifiers alone wait for a key.
function M.hotkey_key(e: { key: string, ctrl: boolean, shift: boolean, alt: boolean, meta: boolean? }): boolean
	local id = state.hotkey_recording:peek()
	if id == "" then
		return false
	end
	local key = e.key
	if key == "Escape" then
		state.hotkey_recording:set("")
		return true
	end
	if key == "Control" or key == "Shift" or key == "Alt" or key == "Meta" then
		return true
	end
	if key == "Backspace" or key == "Delete" then
		host.invoke("set_hotkey", { id = id, keys = "" })
		state.hotkey_recording:set("")
		return true
	end
	local parts = {}
	if e.ctrl then
		table.insert(parts, "Ctrl")
	end
	if e.alt then
		table.insert(parts, "Alt")
	end
	if e.shift then
		table.insert(parts, "Shift")
	end
	if e.meta then
		table.insert(parts, "Win")
	end
	local names = { ArrowUp = "Up", ArrowDown = "Down", ArrowLeft = "Left", ArrowRight = "Right", [" "] = "Space" }
	table.insert(parts, names[key] or (if #key == 1 then string.upper(key) else key))
	host.invoke("set_hotkey", { id = id, keys = table.concat(parts, "+") })
	state.hotkey_recording:set("")
	return true
end

-- The Storage page's Change: a folder dialog, then the meetings there.
function M.choose_storage_folder()
	host.call("choose_storage_folder")
end

-- Make `provider` the main or backup of `job` (nil: no backup).
function M.choose_provider(job: string, slot: "main" | "backup", provider: string?)
	host.invoke("set_job", { job = job, slot = slot, provider = provider or ui.null })
end

-- The Add provider list (Ctrl N).
function M.open_add_provider()
	state.provider_query:set("")
	state.provider_selected:set("")
	state.provider_sheet:set({ screen = "pick", preset = "", id = "" })
end

function M.close_provider_sheet()
	state.provider_sheet:set({ screen = "closed", preset = "", id = "" })
end

-- The form of saved provider `id` (`row`) or of a new one from `preset`.
local function open_form(preset: host.Preset?, row: host.ProviderRow?)
	local f = state.form
	f.key:set("")
	f.name:set(if row then row.name else "")
	f.model:set(if row then row.model elseif preset then preset.model else "")
	f.base_url:set(if row then row.base_url elseif preset then preset.base_url else "")
	f.command:set(if row then row.command elseif preset then preset.command else "")
	f.input:set(if row then row.input else "stdin")
	state.provider_sheet:set({
		screen = "form",
		preset = if row then row.preset elseif preset then preset.id else "",
		id = if row then row.id else "",
	})
end

-- Continue from the list to the selected preset's form.
function M.pick_preset(id: string)
	local preset = providers.preset(host.providers:peek().presets, id)
	if preset then
		open_form(preset, nil)
	end
end

-- The list's selection, moved `delta` rows (the arrows).
function M.move_preset(delta: number)
	local shown = providers.shown(host.providers:peek().presets, state.provider_query:peek())
	if #shown == 0 then
		return
	end
	local at = 1
	for i, preset in shown do
		if preset.id == state.provider_selected:peek() then
			at = i
		end
	end
	at = math.clamp(at + delta, 1, #shown)
	state.provider_selected:set(shown[at].id)
end

-- The list's selected preset (the first shown when none is).
function M.selected_preset(): string
	local shown = providers.shown(host.providers:peek().presets, state.provider_query:peek())
	for _, preset in shown do
		if preset.id == state.provider_selected:peek() then
			return preset.id
		end
	end
	return if shown[1] then shown[1].id else ""
end

-- Edit saved provider `id` (a row's click, Change).
function M.edit_provider(id: string)
	local row = providers.find(host.providers:peek(), id)
	if row then
		open_form(nil, row)
	end
end

-- The open form as a Draft: what it shows, the key only if typed.
function M.draft(): host.Draft
	local sheet = state.provider_sheet:peek()
	local f = state.form
	local key = f.key:peek()
	return {
		id = if sheet.id ~= "" then sheet.id else nil,
		preset = sheet.preset,
		name = f.name:peek(),
		model = f.model:peek(),
		base_url = f.base_url:peek(),
		command = f.command:peek(),
		input = f.input:peek() :: any,
		key = if key ~= "" then key else nil,
	}
end

-- The form's Test button.
function M.test_provider()
	local sheet = state.provider_sheet:peek()
	local draft = M.draft()
	host.call("test_provider", { target = providers.target(sheet.id, sheet.preset), draft = draft })
end

-- The form's Add Provider / Save (Ctrl Enter).
function M.save_provider()
	if state.provider_sheet:peek().screen ~= "form" then
		return
	end
	host.invoke("save_provider", M.draft())
	M.close_provider_sheet()
end

function M.remove_provider(id: string)
	host.invoke("remove_provider", id)
	M.close_provider_sheet()
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
