--!strict
-- The cards of the settings pages (screens/8): General shows You, Captions,
-- Audio and Glossary as the screen does; Captions, Audio and Glossary show
-- their own. Everything reads `host.settings` and changes it through
-- actions (the host saves it and publishes it back).
local host = require("@app/host")
local state = require("@app/state")
local actions = require("@app/actions")
local Icon = require("@components/icon")
local parts = require("@screens/settings/parts")
local controls = require("@screens/settings/controls")

local Section, Row = parts.Section, parts.Row
local Switch, Choice, Stepper, Field = controls.Switch, controls.Choice, controls.Stepper, controls.Field

local M = {}

local function setting(name: string): () -> any
	return function()
		return (host.settings:get() :: any)[name]
	end
end

local function set(name: string): (any) -> ()
	return function(value: any)
		actions.set_settings({ [name] = value })
	end
end

-- "Theme" on the left, its control on the right.
local function Labelled(props: { label: string, first: boolean?, control: Node }): Node
	return Row {
		first = props.first,
		ui.text { props.label, literal = true, grow = 1, basis = 0, truncate = true, text_size = 13, color = "$rc.text.primary" },
		props.control,
	}
end

local function SwitchRow(label: string, field: string, first: boolean?): Node
	return Labelled {
		label = label,
		first = first,
		control = Switch { id = `set-{field}`, label = label, on = setting(field), on_change = set(field) },
	}
end

-- The message under a card when `field`'s last change was refused.
local function Refused(field: string, id: string): Node
	return ui.show(
		function()
			local e = host.settings_error:get()
			return e.field == field and e.message ~= ""
		end,
		ui.text {
			function()
				return host.settings_error:get().message
			end,
			id = id,
			text_size = 12,
			color = "$rc.red",
		}
	)
end

local THEMES = {
	{ value = "system", label = "Follow system" },
	{ value = "dark", label = "Dark" },
	{ value = "light", label = "Light" },
}

function M.You(scope: string): Node
	return Section {
		title = "You",
		Labelled {
			label = "My name",
			first = true,
			control = Field {
				id = "set-my_name",
				label = "My name",
				value = setting("my_name"),
				placeholder = "Your name",
				on_commit = set("my_name"),
			},
		},
		Labelled {
			label = "Theme",
			control = Choice {
				id = "set-theme",
				scope = scope,
				label = "Theme",
				value = setting("theme"),
				options = function()
					return THEMES
				end,
				on_change = set("theme"),
			},
		},
	}
end

function M.Startup(): Node
	return Section { title = "Startup", SwitchRow("Open Recap when Windows starts", "autostart", true) }
end

-- `lines`: the Captions page also sets how many lines the overlay shows.
function M.Captions(lines: boolean): Node
	local rows: { Node } = {
		SwitchRow("Caption my own speech", "caption_my_speech", true),
		Labelled {
			label = "Caption size",
			control = Stepper {
				id = "set-caption_size",
				label = "caption size",
				value = setting("caption_size"),
				min = 14,
				max = 40,
				step = 2,
				unit = "px",
				on_change = set("caption_size"),
			},
		},
	}
	if lines then
		table.insert(
			rows,
			Labelled {
				label = "Caption lines",
				control = Stepper {
					id = "set-caption_lines",
					label = "caption lines",
					value = setting("caption_lines"),
					min = 1,
					max = 3,
					step = 1,
					on_change = set("caption_lines"),
				},
			}
		)
	end
	table.insert(rows, SwitchRow("Show live partial text", "partial_text"))
	table.insert(rows, SwitchRow("Hide overlay from screen share", "hide_from_share"))
	return Section { title = "Captions", rows }
end

-- Apps whose audio alone can be the meeting's.
local APPS = {
	{ value = "system", label = "All system audio" },
	{ value = "ms-teams.exe", label = "Microsoft Teams" },
	{ value = "Zoom.exe", label = "Zoom" },
	{ value = "chrome.exe", label = "Chrome" },
	{ value = "msedge.exe", label = "Edge" },
	{ value = "firefox.exe", label = "Firefox" },
}

function M.Audio(scope: string): Node
	return Section {
		title = "Audio",
		Labelled {
			label = "Microphone",
			first = true,
			control = Choice {
				id = "set-microphone",
				scope = scope,
				label = "Microphone",
				value = setting("microphone"),
				options = function()
					local out = { { value = "", label = "Default" } }
					for _, mic in host.microphones:get() do
						table.insert(out, { value = mic.id, label = mic.name })
					end
					return out
				end,
				on_change = set("microphone"),
			},
		},
		Labelled {
			label = "Meeting audio",
			control = Choice {
				id = "set-meeting_audio",
				scope = scope,
				label = "Meeting audio",
				value = setting("meeting_audio"),
				options = function()
					local chosen = host.settings:get().meeting_audio
					for _, app in APPS do
						if app.value == chosen then
							return APPS
						end
					end
					-- An app set by hand shows as itself.
					local out = table.clone(APPS)
					table.insert(out, { value = chosen, label = chosen })
					return out
				end,
				on_change = set("meeting_audio"),
			},
		},
		SwitchRow("Suggest notes when a call starts", "suggest_on_call"),
	}
end

local function Term(term: string, i: number): Node
	return ui.row {
		h = 28,
		pl = 10,
		pr = 4,
		gap = 2,
		shrink = 0,
		items = "center",
		rounded = 999,
		bg = "$rc.fill.selected",
		ui.text { term, id = `term-{i}`, literal = true, text_size = 13, color = "$rc.text.primary" },
		ui.button {
			id = `term-{i}-remove`,
			label = `Remove {term}`,
			w = 20,
			h = 20,
			p = 0,
			items = "center",
			justify = "center",
			rounded = 999,
			bg = "$rc.transparent",
			hover = { bg = "$rc.fill.hover" },
			on_click = function()
				actions.remove_term(term)
			end,
			Icon { name = "close", size = 12, color = "$rc.text.tertiary" },
		},
	}
end

-- "+ Add": where a new term is typed (Enter adds it).
local function AddTerm(): Node
	return ui.show(
		function()
			return #host.settings:get().glossary < 50
		end,
		ui.row {
			h = 28,
			pl = 8,
			pr = 10,
			gap = 4,
			shrink = 0,
			items = "center",
			rounded = 999,
			border = 1,
			border_color = "$rc.stroke.field",
			border_dash = { 3, 3 },
			Icon { name = "plus", size = 14, color = "$rc.text.tertiary" },
			ui.input {
				id = "glossary-input",
				label = "Add a glossary term",
				value = state.glossary_draft,
				placeholder = "Add",
				placeholder_color = "$rc.text.tertiary",
				on_commit = function()
					actions.add_term()
				end,
				w = 120,
				h = 24,
				p = 0,
				text_size = 13,
				bg = "$rc.transparent",
				border = 0,
			},
		}
	)
end

function M.Glossary(): Node
	return ui.column {
		w_full = true,
		gap = 6,
		Section {
			title = "Glossary",
			ui.row {
				id = "glossary-terms",
				w_full = true,
				py = 12,
				gap = 8,
				flex_wrap = true,
				ui.each(
					ui.computed(function()
						return host.settings:get().glossary
					end),
					function(term: string, i: number)
						return Term(term, i)
					end
				),
				AddTerm(),
			},
		},
		Refused("glossary", "glossary-error"),
	}
end

return M
