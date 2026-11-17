--!strict
-- A provider's form (screens/8d API, 8e custom command): the fields its
-- preset asks for, Test with its time or error, and Add Provider / Save
-- (Ctrl Enter). Follows state.provider_sheet: one form for every preset.
local host = require("@app/host")
local state = require("@app/state")
local actions = require("@app/actions")
local providers = require("@lib/providers")
local Icon = require("@components/icon")
local Logo = require("@components/logo")
local Button = require("@components/button")

export type Props = { back: () -> Node }

-- What the open form is about: the saved provider's row, or its preset.
type Subject = {
	title: string,
	logo: string,
	fields: host.Fields,
	key_hint: string,
	key_mask: string,
	-- A saved AI provider (it can be removed).
	removable: boolean,
	saved: boolean,
	placeholder_base_url: string,
}

local NO_FIELDS = { key = false, model = false, base_url = false, command = false, name = false }

local function subject(): Subject
	local sheet = state.provider_sheet:get()
	local view = host.providers:get()
	local row = if sheet.id ~= "" then providers.find(view, sheet.id) else nil
	local preset = providers.preset(view.presets, sheet.preset)
	if row then
		return {
			title = row.name,
			logo = row.logo,
			fields = row.fields,
			key_hint = row.key_hint,
			key_mask = row.key_mask,
			removable = row.family == "ai",
			saved = true,
			placeholder_base_url = if preset then preset.base_url else "",
		}
	end
	return {
		title = if preset then preset.name else "",
		logo = if preset then preset.logo else "compatible",
		fields = if preset then preset.fields else NO_FIELDS,
		key_hint = if preset then preset.key_hint else "",
		key_mask = "",
		removable = false,
		saved = false,
		placeholder_base_url = if preset then preset.base_url else "",
	}
end

local function Label(text: string): Node
	return ui.text { text, text_size = 12, color = "$rc.text.secondary" }
end

local function field_box(props: { [string]: any }): Node
	local base = {
		w_full = true,
		h = 36,
		px = 12,
		text_size = 13,
		rounded = 6,
		bg = "$rc.fill.input",
		border = 1,
		border_color = "$rc.stroke.field",
		placeholder_color = "$rc.text.tertiary",
	}
	for k, v in props do
		base[k] = v
	end
	return ui.input(base :: any)
end

local function Field(show: (host.Fields) -> boolean, label: string, input: Node): Node
	return ui.show(function()
		return show(subject().fields)
	end, ui.column { w_full = true, gap = 8, Label(label), input })
end

local TOKENS = { "prompt", "schema", "transcript_file" }

local function Tokens(): Node
	local chips = {}
	for _, token in TOKENS do
		table.insert(
			chips,
			ui.button {
				id = `token-{token}`,
				h = 24,
				px = 8,
				rounded = 4,
				bg = "$rc.accent.soft",
				hover = { bg = "$rc.fill.hover" },
				on_click = function()
					state.form.command:set(state.form.command:peek() .. `\{{token}\}`)
				end,
				ui.text { token, literal = true, text_size = 12, font = "$rc.font.mono", color = "$rc.accent" },
			}
		)
	end
	return ui.row { gap = 8, chips }
end

local function TestRow(): Node
	local test = ui.computed(function(): host.ProviderTest?
		local sheet = state.provider_sheet:get()
		local t = host.provider_test:get()
		return if t.target == providers.target(sheet.id, sheet.preset) and t.status ~= "idle" then t else nil
	end)
	return ui.row {
		gap = 12,
		items = "center",
		Button { id = "form-test", label = "Test", icon = "play", look = "tinted", on_click = actions.test_provider },
		ui.show(
			function()
				local t = test:get()
				return t ~= nil and t.status == "ok"
			end,
			Icon { id = "form-test-ok", name = "check", color = "$rc.text.secondary" }
		),
		ui.show(
			function()
				return test:get() ~= nil
			end,
			ui.text {
				function()
					local t = test:get()
					if t == nil then
						return ""
					elseif t.status == "running" then
						return "Testing…"
					elseif t.status == "ok" then
						return providers.latency(t.latency_ms)
					end
					return t.error
				end,
				id = "form-test-result",
				literal = true,
				text_size = 13,
				color = function()
					local t = test:get()
					return if t and t.status == "error" then "$rc.red" else "$rc.text.secondary"
				end,
			}
		),
	}
end

local function Footer(): Node
	return ui.column {
		w_full = true,
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
		ui.row {
			h = 42,
			w_full = true,
			pl = 16,
			pr = 12,
			gap = 8,
			items = "center",
			ui.row {
				w = 16,
				h = 16,
				ui.each(
					ui.computed(function()
						return { subject().logo }
					end),
					function(_, _, logo)
						return Logo { name = logo:peek() }
					end,
					{
						key = function(logo: string): string
							return logo
						end,
					}
				),
			},
			ui.text {
				function()
					return subject().title
				end,
				literal = true,
				grow = 1,
				basis = 0,
				truncate = true,
				text_size = 13,
				color = "$rc.text.secondary",
			},
			ui.show(
				function()
					return subject().removable
				end,
				ui.row {
					items = "center",
					gap = 4,
					Button {
						id = "form-remove",
						label = "Remove",
						color = "$rc.red",
						on_click = function()
							actions.remove_provider(state.provider_sheet:peek().id)
						end,
					},
					ui.div { w = 1, h = 16, mx = 4, bg = "$rc.stroke.hairline" },
				}
			),
			Button {
				label = function(): string
					return if subject().saved then "Save" else "Add Provider"
				end,
				label_id = "form-save",
				keys = { "Ctrl", "↵" },
				on_click = actions.save_provider,
			},
		},
	}
end

return function(props: Props): Node
	return ui.column {
		id = "provider-form",
		w_full = true,
		on_key = function(e)
			if e.ctrl and e.key == "Enter" then
				actions.save_provider()
				return true
			end
			return false
		end,
		ui.row {
			h = 64,
			w_full = true,
			shrink = 0,
			px = 12,
			gap = 12,
			items = "center",
			props.back(),
			ui.each(
				ui.computed(function()
					return { subject().logo }
				end),
				function(_, _, logo)
					return Logo { name = logo:peek(), tile = 28 }
				end,
				{
					key = function(logo: string): string
						return logo
					end,
				}
			),
			ui.text {
				function()
					return subject().title
				end,
				id = "form-title",
				literal = true,
				text_size = 18,
				color = "$rc.text.primary",
			},
		},
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
		ui.column {
			w_full = true,
			px = 24,
			pt = 20,
			pb = 24,
			gap = 18,
			Field(function(f)
				return f.name
			end, "Name", field_box { id = "form-name", label = "Name", value = state.form.name }),
			Field(
				function(f)
					return f.key
				end,
				"API key",
				field_box {
					id = "form-key",
					label = "API key",
					value = state.form.key,
					secure = true,
					font = "$rc.font.mono",
					placeholder = function()
						local s = subject()
						return if s.key_mask ~= "" then s.key_mask else s.key_hint
					end,
				}
			),
			Field(function(f)
				return f.model
			end, "Model", field_box { id = "form-model", label = "Model", value = state.form.model }),
			Field(
				function(f)
					return f.base_url
				end,
				"Base URL",
				field_box {
					id = "form-base-url",
					label = "Base URL",
					value = state.form.base_url,
					font = "$rc.font.mono",
					placeholder = function()
						return subject().placeholder_base_url
					end,
				}
			),
			ui.show(function()
				return subject().fields.command
			end, ui.column {
				w_full = true,
				gap = 18,
				ui.column {
					w_full = true,
					gap = 8,
					Label("Command"),
					field_box { id = "form-command", label = "Command", value = state.form.command, font = "$rc.font.mono" },
					Tokens(),
				},
				ui.column {
					gap = 8,
					Label("Transcript"),
					ui.segmented {
						id = "form-input",
						label = "Transcript",
						value = state.form.input,
						options = { "stdin", "file" },
					},
				},
			}),
			TestRow(),
		},
		Footer(),
	}
end
