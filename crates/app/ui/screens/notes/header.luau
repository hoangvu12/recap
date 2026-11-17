--!strict
-- The Notes window's header: the meeting's title, its date chip, and the
-- Notes / Transcript / Summary tabs over a hairline. It moves the window.
local host = require("@app/host")
local state = require("@app/state")
local actions = require("@app/actions")
local format = require("@lib/format")
local processing = require("@lib/processing")
local Icon = require("@components/icon")
local IconButton = require("@components/icon_button")

local function tab(id: state.NotesTab, label: string): Node
	local selected = function()
		return state.notes_tab:get() == id
	end
	-- Summary waits for the meeting to be processed (screens/16: dimmed
	-- while the card works, unless a summary is already there).
	local disabled = function()
		if id ~= "summary" then
			return false
		end
		if host.meeting:get().state ~= "stopped" then
			return true
		end
		return processing.working(host.processing:get()) and host.summary:get().id == ""
	end
	return ui.button {
		id = `tab-{id}`,
		role = "tab",
		selected = selected,
		disabled = disabled,
		h = 34,
		bg = "$rc.transparent",
		hover = { bg = "$rc.transparent" },
		p = 0,
		rounded = 0,
		focus = { border = 0 },
		on_click = function()
			actions.show_tab(id)
		end,
		ui.column {
			h_full = true,
			justify = "between",
			ui.text {
				label,
				text_size = 13,
				weight = 500,
				color = function()
					if disabled() then
						return "$rc.text.quaternary"
					end
					return if selected() then "$rc.text.primary" else "$rc.text.secondary"
				end,
				pt = 6,
			},
			ui.div {
				h = 2,
				w_full = true,
				rounded = 1,
				bg = function()
					return if selected() then "$rc.accent" else "$rc.transparent"
				end,
			},
		},
	}
end

return function(): Node
	return ui.column {
		id = "notes-header",
		w_full = true,
		shrink = 0,
		window_drag = true,
		ui.column {
			px = 16,
			pt = 12,
			gap = 4,
			ui.row {
				w_full = true,
				gap = 8,
				items = "center",
				ui.text {
					function()
						local title = host.meeting:get().title
						return if title == "" then "New meeting" else title
					end,
					id = "meeting-title",
					grow = 1,
					basis = 0,
					text_size = 18,
					color = function()
						return if host.meeting:get().title == "" then "$rc.text.secondary" else "$rc.text.primary"
					end,
					truncate = true,
				},
				IconButton {
					id = "notes-settings",
					icon = "settings",
					label = "Settings",
					keys = { "Ctrl", "," },
					on_click = actions.open_settings,
					tip_side = "bottom",
				},
			},
			ui.row {
				id = "meeting-date",
				gap = 4,
				items = "center",
				h = 20,
				Icon { name = "calendar", size = 14 },
				ui.text {
					function()
						local started = host.meeting:get().started_ms
						return if started > 0 then format.date(started) else ""
					end,
					text_size = 13,
					color = "$rc.text.secondary",
				},
			},
			ui.row {
				role = "tablist",
				mt = 14,
				gap = 20,
				tab("notes", "Notes"),
				tab("transcript", "Transcript"),
				tab("summary", "Summary"),
			},
		},
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
	}
end
