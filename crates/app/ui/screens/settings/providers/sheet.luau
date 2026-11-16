--!strict
-- The Add provider sheet over Settings: the preset list (screens/8c) and a
-- provider's form (8d API, 8e custom command), a Raycast panel each with a
-- header, a body and a footer action bar.
local host = require("@app/host")
local state = require("@app/state")
local actions = require("@app/actions")
local providers = require("@lib/providers")
local Icon = require("@components/icon")
local Logo = require("@components/logo")
local Keycap = require("@components/keycap")
local Form = require("./form")

local function back()
	local sheet = state.provider_sheet:peek()
	if sheet.screen == "form" and sheet.id == "" then
		state.provider_sheet:set({ screen = "pick", preset = "", id = "" })
	else
		actions.close_provider_sheet()
	end
end

local function BackButton(): Node
	return ui.button {
		id = "sheet-back",
		label = "Back",
		w = 28,
		h = 28,
		p = 0,
		shrink = 0,
		items = "center",
		justify = "center",
		rounded = 6,
		bg = "$rc.transparent",
		hover = { bg = "$rc.fill.hover" },
		on_click = back,
		Icon { name = "chevron-left", color = "$rc.text.secondary" },
	}
end

local function PresetRow(preset: host.Preset): Node
	local selected = function()
		state.provider_query:get()
		state.provider_selected:get()
		return actions.selected_preset() == preset.id
	end
	return ui.show(
		function()
			local q = string.lower(state.provider_query:get())
			return q == "" or string.find(string.lower(preset.name), q, 1, true) ~= nil
		end,
		ui.button {
			id = `preset-{preset.id}`,
			role = "option",
			selected = selected,
			h = 38,
			w_full = true,
			shrink = 0,
			p = 0,
			rounded = 8,
			bg = "$rc.transparent",
			hover = { bg = "$rc.fill.hover" },
			on_click = function()
				state.provider_selected:set(preset.id)
			end,
			on_double_click = function()
				actions.pick_preset(preset.id)
			end,
			ui.row {
				w_full = true,
				h_full = true,
				px = 8,
				gap = 12,
				items = "center",
				rounded = 8,
				bg = function()
					return if selected() then "$rc.fill.selected" else "$rc.transparent"
				end,
				Logo { name = preset.logo, tile = 22 },
				ui.text {
					preset.name,
					id = `preset-{preset.id}-name`,
					literal = true,
					grow = 1,
					basis = 0,
					text_size = 13,
					color = "$rc.text.primary",
				},
				ui.text {
					preset.detail,
					id = `preset-{preset.id}-detail`,
					literal = true,
					shrink = 0,
					text_size = 12,
					font = "$rc.font.mono",
					color = "$rc.text.tertiary",
				},
			},
		}
	)
end

local GROUPS = {
	{ id = "api", label = "API" },
	{ id = "local", label = "On this PC" },
	{ id = "cli", label = "Command line" },
	{ id = "custom", label = "Custom" },
}

local function List(): Node
	local presets = host.providers:peek().presets
	local sections = {}
	for _, group in GROUPS do
		local items = {}
		for _, preset in presets do
			if preset.group == group.id then
				table.insert(items, PresetRow(preset))
			end
		end
		if #items > 0 then
			local label = group.label
			local index = #sections + 1
			table.insert(
				sections,
				ui.show(
					function()
						for _, section in providers.sections(host.providers:get().presets, state.provider_query:get()) do
							if section.label == label then
								return true
							end
						end
						return false
					end,
					ui.column {
						w_full = true,
						ui.row {
							h = 30,
							px = 8,
							pb = 6,
							items = "end",
							ui.text { label, id = `preset-section-{index}`, text_size = 11, weight = 500, color = "$rc.text.secondary" },
						},
						items,
					}
				)
			)
		end
	end
	-- The search keeps focus; its arrows move the selection, Enter goes on.
	local function keys(e): boolean
		if e.key == "ArrowDown" or e.key == "ArrowUp" then
			actions.move_preset(if e.key == "ArrowDown" then 1 else -1)
			return true
		end
		if e.key == "Enter" then
			actions.pick_preset(actions.selected_preset())
			return true
		end
		return false
	end
	return ui.column {
		id = "provider-list",
		w_full = true,
		on_key = keys,
		ui.row {
			h = 56,
			w_full = true,
			shrink = 0,
			px = 12,
			gap = 8,
			items = "center",
			BackButton(),
			ui.input {
				id = "provider-search",
				label = "Search providers",
				value = state.provider_query,
				on_key = keys,
				on_mount = function(el)
					el:focus()
				end,
				placeholder = "Search providers…",
				placeholder_color = "$rc.text.tertiary",
				grow = 1,
				basis = 0,
				text_size = 18,
				bg = "$rc.transparent",
				border = 0,
				p = 0,
			},
		},
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
		ui.scroll {
			h = 440,
			w_full = true,
			ui.column { w_full = true, px = 8, pb = 8, sections },
		},
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
		ui.row {
			h = 42,
			w_full = true,
			pl = 16,
			pr = 12,
			gap = 8,
			items = "center",
			Icon { name = "settings" },
			ui.text { "Add provider", grow = 1, text_size = 13, color = "$rc.text.secondary" },
			ui.button {
				id = "continue",
				h = 28,
				px = 6,
				gap = 8,
				items = "center",
				rounded = 6,
				bg = "$rc.transparent",
				hover = { bg = "$rc.fill.hover" },
				on_click = function()
					actions.pick_preset(actions.selected_preset())
				end,
				ui.text { "Continue", text_size = 13, weight = 500, color = "$rc.text.primary" },
				Keycap("↵"),
			},
		},
	}
end

return function(): Node
	return ui.modal {
		open = function()
			return state.provider_sheet:get().screen ~= "closed"
		end,
		on_close = actions.close_provider_sheet,
		label = "Add provider",
		w = 640,
		p = 0,
		gap = 0,
		rounded = 12,
		bg = "$rc.bg.window",
		border = 1,
		border_color = "$rc.stroke.hairline",
		build = function(): Node
			return ui.switch(function()
				return state.provider_sheet:get().screen
			end, {
				pick = List(),
				form = Form { back = BackButton },
			}, ui.column {})
		end,
	}
end
