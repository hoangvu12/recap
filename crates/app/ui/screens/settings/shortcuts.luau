--!strict
-- Settings, Shortcuts: the global hotkeys. Click one and press the new
-- keys (Esc keeps it, Backspace clears it); the host refuses keys without
-- a modifier or already in use, and says so under the card.
local host = require("@app/host")
local state = require("@app/state")
local actions = require("@app/actions")
local Keycap = require("@components/keycap")
local parts = require("@screens/settings/parts")

local HOTKEYS = {
	{ id = "start_notes", label = "Start notes" },
	{ id = "pause", label = "Pause or resume" },
	{ id = "click_through", label = "Click-through" },
	{ id = "captions", label = "Show or hide captions" },
}

local function keys(id: string): string
	return (host.settings:get().hotkeys :: any)[id] or ""
end

local function Keys(id: string): Node
	local recording = function()
		return state.hotkey_recording:get() == id
	end
	return ui.button {
		id = `hotkey-{id}`,
		label = `Change`,
		h = 28,
		px = 6,
		gap = 4,
		shrink = 0,
		items = "center",
		rounded = 6,
		bg = function()
			return if recording() then "$rc.fill.selected" else "$rc.transparent"
		end,
		border = 1,
		border_color = function()
			return if recording() then "$rc.stroke.focus" else "$rc.transparent"
		end,
		hover = { bg = "$rc.fill.hover" },
		on_click = function()
			actions.record_hotkey(id)
		end,
		ui.show(
			recording,
			ui.text { "Press keys", id = `hotkey-{id}-recording`, text_size = 13, color = "$rc.text.secondary" },
			ui.row {
				gap = 4,
				items = "center",
				ui.each(
					ui.computed(function(): { string }
						local k = keys(id)
						return if k == "" then {} else string.split(k, "+")
					end),
					function(key: string)
						return Keycap(key)
					end
				),
				ui.show(function()
					return keys(id) == ""
				end, ui.text { "None", text_size = 13, color = "$rc.text.tertiary" }),
				-- The whole hotkey, for tests and assistive technology; not drawn.
				ui.text { function()
					return keys(id)
				end, id = `hotkey-{id}-keys`, w = 0, h = 0, overflow = "clip", opacity = 0 },
			}
		),
	}
end

-- Under the card: why a change was refused, and a hotkey another app holds.
local function Refused(id: string): { Node }
	local field = `hotkey.{id}`
	local function taken(): string
		return host.hotkey_conflicts:get()[id] or ""
	end
	return {
		ui.show(
			function()
				local e = host.settings_error:get()
				return e.field == field and e.message ~= ""
			end,
			ui.text {
				function()
					return host.settings_error:get().message
				end,
				id = `hotkey-{id}-error`,
				text_size = 12,
				color = "$rc.red",
			}
		),
		-- Saved, but the system gave it to another app first.
		ui.show(function()
			return taken() ~= ""
		end, ui.text { taken, id = `hotkey-{id}-taken`, text_size = 12, color = "$rc.red" }),
	}
end

return function(): Node
	local rows = {}
	local errors = {}
	for i, h in HOTKEYS do
		table.insert(
			rows,
			parts.Row {
				id = `hotkey-row-{h.id}`,
				first = i == 1,
				ui.text { h.label, grow = 1, basis = 0, text_size = 13, color = "$rc.text.primary" },
				Keys(h.id),
			}
		)
		for _, node in Refused(h.id) do
			table.insert(errors, node)
		end
	end
	return ui.column {
		id = "page-shortcuts",
		w_full = true,
		gap = 6,
		parts.Section { title = "Shortcuts", rows },
		errors,
	}
end
