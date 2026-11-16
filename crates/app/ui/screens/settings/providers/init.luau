--!strict
-- Settings, Providers (screens/8b): each job's main and backup, the speech
-- engines with what they can do and their key, the AI providers, and Add
-- provider (Ctrl N) with its sheet (8c–8e). Everything here is drawn from
-- host.providers: a provider added to the registry shows with no UI work.
local host = require("@app/host")
local actions = require("@app/actions")
local providers = require("@lib/providers")
local Icon = require("@components/icon")
local Logo = require("@components/logo")
local Keycap = require("@components/keycap")
local parts = require("@screens/settings/parts")
local Picker = require("@self/picker")
local Sheet = require("@self/sheet")

local function UseFor(): Node
	local function JobRow(job: host.JobChoice, first: boolean): Node
		return parts.Row {
			id = `job-{job.id}`,
			first = first,
			ui.text {
				job.label,
				id = `job-{job.id}-label`,
				w = 160,
				shrink = 0,
				text_size = 13,
				color = "$rc.text.primary",
			},
			Picker { job = job.id, slot = "main" },
			Picker { job = job.id, slot = "backup" },
		}
	end
	return parts.Section {
		title = "Use for",
		id = "use-for",
		ui.row {
			h = 28,
			w_full = true,
			gap = 10,
			items = "end",
			pb = 4,
			ui.div { w = 160, shrink = 0 },
			ui.text { "Main", grow = 1, basis = 0, text_size = 12, color = "$rc.text.secondary" },
			ui.text { "Backup", grow = 1, basis = 0, text_size = 12, color = "$rc.text.secondary" },
		},
		ui.each(
			ui.computed(function()
				return host.providers:get().jobs
			end),
			function(_, _, job)
				-- A hairline under the Main / Backup heading too.
				return JobRow(job:peek(), false)
			end,
			{
				key = function(job: host.JobChoice): string
					return job.id
				end,
				w_full = true,
			}
		),
	}
end

local function SpeechRow(row: State<host.ProviderRow>, first: boolean): Node
	local id = row:peek().id
	local abilities = {}
	for i, label in providers.abilities(row:peek()) do
		table.insert(abilities, parts.Tag(label, `stt-{id}-ability-{i}`))
	end
	return parts.Row {
		id = `stt-{id}`,
		first = first,
		h = 52,
		Logo { name = row:peek().logo, tile = 28 },
		ui.text {
			function()
				return row:get().name
			end,
			id = `stt-{id}-name`,
			literal = true,
			shrink = 0,
			text_size = 13,
			weight = 500,
			color = "$rc.text.primary",
		},
		ui.row { gap = 6, shrink = 0, items = "center", abilities },
		ui.div { grow = 1 },
		ui.text {
			function()
				return row:get().key_mask
			end,
			id = `stt-{id}-key`,
			literal = true,
			shrink = 0,
			text_size = 12,
			font = "$rc.font.mono",
			color = "$rc.text.secondary",
		},
		ui.button {
			id = `stt-{id}-change`,
			h = 28,
			px = 10,
			shrink = 0,
			rounded = 6,
			bg = "$rc.transparent",
			border = 1,
			border_color = "$rc.stroke.field",
			hover = { bg = "$rc.fill.hover" },
			on_click = function()
				actions.edit_provider(id)
			end,
			ui.text {
				function()
					return if row:get().key_mask ~= "" then "Change" else "Add key"
				end,
				text_size = 13,
				weight = 500,
				color = "$rc.text.primary",
			},
		},
	}
end

local function AiRow(row: State<host.ProviderRow>, first: boolean): Node
	local id = row:peek().id
	return ui.column {
		w_full = true,
		ui.div { h = if first then 0 else 1, w_full = true, bg = "$rc.stroke.hairline" },
		ui.button {
			id = `ai-{id}`,
			h = 52,
			w_full = true,
			shrink = 0,
			px = 0,
			gap = 10,
			items = "center",
			rounded = 6,
			bg = "$rc.transparent",
			hover = { bg = "$rc.fill.hover" },
			on_click = function()
				actions.edit_provider(id)
			end,
			Logo { name = row:peek().logo, tile = 28 },
			ui.text {
				function()
					return row:get().name
				end,
				id = `ai-{id}-name`,
				literal = true,
				shrink = 0,
				text_size = 13,
				weight = 500,
				color = "$rc.text.primary",
			},
			ui.text {
				function()
					return row:get().subtitle
				end,
				id = `ai-{id}-subtitle`,
				literal = true,
				grow = 1,
				basis = 0,
				truncate = true,
				text_size = 12,
				font = "$rc.font.mono",
				color = "$rc.text.tertiary",
			},
			ui.text {
				function()
					return row:get().trailing
				end,
				id = `ai-{id}-trailing`,
				literal = true,
				shrink = 0,
				text_size = 12,
				font = "$rc.font.mono",
				color = "$rc.text.secondary",
			},
			ui.show(
				function()
					local tested = row:get().tested
					return tested ~= nil and tested.ok
				end,
				Icon { id = `ai-{id}-tested`, name = "check", color = "$rc.text.secondary" }
			),
		},
	}
end

local function AddRow(): Node
	return ui.column {
		w_full = true,
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
		ui.button {
			id = "add-provider",
			h = 52,
			w_full = true,
			px = 0,
			gap = 10,
			items = "center",
			rounded = 6,
			bg = "$rc.transparent",
			hover = { bg = "$rc.fill.hover" },
			on_click = actions.open_add_provider,
			ui.row {
				w = 28,
				h = 28,
				shrink = 0,
				items = "center",
				justify = "center",
				rounded = 6,
				border = 1,
				border_dash = { 3, 3 },
				border_color = "$rc.stroke.field",
				Icon { name = "plus", color = "$rc.text.secondary" },
			},
			ui.text { "Add provider", grow = 1, text_size = 13, weight = 500, color = "$rc.text.primary" },
			ui.row { gap = 4, Keycap("Ctrl"), Keycap("N") },
		},
	}
end

local function rows(list: () -> { host.ProviderRow }, build: (State<host.ProviderRow>, boolean) -> Node): Node
	return ui.each(ui.computed(list), function(_, index, row)
		return build(row, index == 1)
	end, {
		key = function(row: host.ProviderRow): string
			return row.id
		end,
		w_full = true,
	})
end

return function(): Node
	return ui.column {
		id = "providers-page",
		w_full = true,
		gap = 28,
		UseFor(),
		parts.Section {
			title = "Speech-to-text",
			id = "speech",
			rows(function()
				return host.providers:get().stt
			end, SpeechRow),
		},
		parts.Section {
			title = "AI",
			id = "ai",
			rows(function()
				return host.providers:get().ai
			end, AiRow),
			AddRow(),
		},
		Sheet(),
	}
end
