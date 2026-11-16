--!strict
-- What the user can do: buttons, keys and later the action panel call these,
-- so the same action behind each is one function.
local host = require("@app/host")
local state = require("@app/state")
local meetings = require("@lib/meetings")
local providers = require("@lib/providers")
local processing = require("@lib/processing")
local format = require("@lib/format")

local M = {}

-- Stop the meeting (Stop ↵). My notes are kept first: they steer the summary.
function M.stop()
	local m = host.meeting:peek()
	if m.state == "recording" or m.state == "paused" then
		local notes = state.my_notes:peek()
		if m.id and m.id ~= "" and string.match(notes, "%S") then
			host.invoke("set_my_notes", { id = m.id, text = notes })
		end
		host.invoke("stop")
	end
end

-- Tick next step `index` (from 1) of the summary shown on or off.
function M.tick_next_step(index: number, done: boolean)
	local s = host.summary:peek()
	if s.id ~= "" then
		host.invoke("tick_next_step", { id = s.id, index = index, done = done })
	end
end

-- Name the speakers still unnamed and write the shown meeting's summary
-- again (Regenerate summary).
function M.regenerate_summary()
	local id = host.meeting:peek().id
	if id and id ~= "" and host.meeting:peek().state == "stopped" then
		host.invoke("regenerate_summary", id)
	end
end

-- The footer's primary action (↵): Stop while recording, Resume while
-- paused, Retry when a processing step failed, Copy Markdown once done
-- (screens/9).
function M.primary()
	local s = host.meeting:peek().state
	if s == "paused" then
		host.invoke("resume")
	elseif s == "recording" then
		M.stop()
	elseif processing.failed(host.processing:peek()) then
		M.retry_processing()
	elseif M.can_copy_notes() then
		M.run_action("notes", "copy_markdown")
	end
end

-- Whether the Notes window's meeting is stored and done processing, so
-- Copy Markdown is its primary action.
function M.can_copy_notes(): boolean
	return not processing.active(host.processing:peek()) and M.action_target("notes") ~= nil
end

-- Process the meeting again from the step that failed (Retry).
function M.retry_processing()
	local p = host.processing:peek()
	if p.id ~= "" then
		host.invoke("retry_processing", p.id)
	end
end

-- Close the Notes window while it processes (Hide Esc); it goes on.
function M.hide_notes()
	host.windows.notes:set(false)
end

-- "Use API key…": the providers, to fix what failed.
function M.fix_providers()
	M.open_settings("providers")
end

function M.toggle_pause()
	local s = host.meeting:peek().state
	if s == "recording" then
		host.invoke("pause")
	elseif s == "paused" then
		host.invoke("resume")
	end
end

-- The overlay ----------------------------------------------------------------

function M.open_notes()
	host.windows.notes:set(true)
end

-- Caption text `delta` points larger (−/+ in the overlay's toolbar), within
-- Settings' 14–40.
function M.caption_size(delta: number)
	local size = host.settings:peek().caption_size
	local next = math.clamp(size + delta, 14, 40)
	if next ~= size then
		host.invoke("set_settings", { caption_size = next })
	end
end

-- Show my own lines in the captions, or no longer.
function M.toggle_my_lines()
	host.invoke("set_settings", { caption_my_speech = not host.settings:peek().caption_my_speech })
end

-- Keep the overlay above every window, or no longer.
function M.toggle_pinned()
	host.invoke("set_overlay_pinned", not host.overlay:peek().pinned)
end

-- Clicks go through the overlay (its hotkey does the same).
function M.toggle_click_through()
	host.invoke("toggle_click_through")
end

-- Caption with `choice` where the live engine failed ("Switch to Deepgram").
function M.switch_live(choice: host.LiveChoice)
	host.invoke("switch_live", { id = choice.id, name = choice.name })
end

-- Action panel (Ctrl K) -----------------------------------------------------

-- The meeting the action panel of window `where` acts on: Home's selected
-- one; the Notes window's once stopped and stored. Nil: none.
export type Target = { id: string, title: string, started_ms: number }
function M.action_target(where: state.Where | ""): Target?
	if where == "home" then
		local id = meetings.pick(M.home_shown(), state.home_selected:peek())
		for _, item in host.meetings:peek() do
			if item.id == id then
				return { id = id, title = meetings.title(item), started_ms = item.started_ms }
			end
		end
	elseif where == "notes" then
		local m = host.meeting:peek()
		if m.state == "stopped" and m.id and m.id ~= "" then
			return { id = m.id, title = if m.title ~= "" then m.title else "Untitled meeting", started_ms = m.started_ms }
		end
	end
	return nil
end

-- Open or close window `where`'s action panel (Ctrl K); it opens on a
-- meeting only.
function M.toggle_actions(where: state.Where)
	if state.actions_open:peek() == where then
		M.close_actions()
	elseif M.action_target(where) then
		state.actions_query:set("")
		state.actions_selected:set("")
		state.actions_open:set(where)
	end
end

function M.close_actions()
	state.actions_open:set("")
end

-- Show `text` in the HUD pill for 1.2 s (motion_spec §2.8).
function M.hud(text: string)
	local seq = state.hud:peek().seq + 1
	state.hud:set({ text = text, seq = seq })
	ui.after(1200, function()
		if state.hud:peek().seq == seq then
			state.hud:set({ text = "", seq = seq })
		end
	end)
end

local function markdown(target: Target): host.MeetingMarkdown?
	local md = host.markdown(target.id, format.date(target.started_ms))
	if not md then
		M.hud("Can't read the meeting")
	end
	return md
end

-- Run action `id` (lib/meeting_actions) on window `where`'s meeting.
function M.run_action(where: state.Where, id: string)
	local target = M.action_target(where)
	M.close_actions()
	if not target then
		return
	end
	if id == "copy_markdown" then
		local md = markdown(target)
		if md then
			ui.clipboard.write(md.markdown)
			M.hud("Copied")
		end
	elseif id == "export_markdown" then
		local md = markdown(target)
		if md then
			ui.save_file({ title = "Export .md", name = md.file_name, filters = { { "Markdown", { "md" } } } }):after(function(file)
				if file then
					ui.write_file(file, md.markdown):after(function()
						M.hud("Exported")
					end, function()
						M.hud("Can't export")
					end)
				end
			end, function() end)
		end
	elseif id == "name_speakers" then
		-- The Name speakers dialog (ticket #12) is the host's.
		host.invoke("open_name_speakers", target.id)
	elseif id == "regenerate_summary" then
		host.invoke("regenerate_summary", target.id)
	elseif id == "show_in_folder" then
		host.invoke("show_in_folder", target.id)
	elseif id == "delete_meeting" then
		state.confirm_delete:set({ id = target.id, title = target.title, where = where })
	end
end

-- Delete meeting, confirmed.
function M.confirm_delete()
	local c = state.confirm_delete:peek()
	if c.id ~= "" then
		host.invoke("delete_meeting", c.id)
	end
	state.confirm_delete:set({ id = "", title = "", where = "" })
end

function M.cancel_delete()
	state.confirm_delete:set({ id = "", title = "", where = "" })
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
	local query = state.home_query:peek()
	return meetings.visible(host.meetings:peek(), query, host.search(query))
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
