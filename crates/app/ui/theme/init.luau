--!strict
-- The app's two themes, from the generated tokens: recap-dark (the default)
-- and recap-light, a token swap. Our tokens are `$rc.*`; zgx's standard ones
-- follow them so its own widgets (inputs, scrollbars, focus rings) match.
local tokens = require("@self/tokens")

local M = {}

M.DARK = "recap-dark"
M.LIGHT = "recap-light"

-- zgx's standard tokens in terms of ours.
local function standard(t: { [string]: string }): { [string]: string | number }
	return {
		background = t["rc.bg.window"],
		["surface.base"] = t["rc.bg.window"],
		["surface.raised"] = t["rc.bg.raised"],
		["surface.overlay"] = t["rc.bg.raised"],
		["text.primary"] = t["rc.text.primary"],
		["text.secondary"] = t["rc.text.secondary"],
		["text.tertiary"] = t["rc.text.tertiary"],
		["text.disabled"] = t["rc.text.quaternary"],
		accent = t["rc.accent"],
		["accent.text"] = t["rc.on.accent"],
		border = t["rc.stroke.field"],
		["border.subtle"] = t["rc.stroke.hairline"],
		["border.strong"] = t["rc.stroke.focus"],
		hover = t["rc.fill.hover"],
		["selection.item"] = t["rc.fill.selected"],
		["focus.ring"] = t["rc.stroke.focus"],
		danger = t["rc.red"],
		success = t["rc.green"],
		warning = t["rc.orange"],
		info = t["rc.blue"],
		["input.bg"] = t["rc.fill.input"],
		["font.sans"] = t["rc.font.ui"],
		["font.mono"] = t["rc.font.mono"],
	}
end

local function merged(t: { [string]: string }): { [string]: string | number }
	local out: { [string]: string | number } = standard(t)
	for name, value in t do
		out[name] = value
	end
	return out
end

-- Declare our tokens and register both themes; dark unless chosen otherwise.
function M.setup()
	ui.theme.tokens(tokens.dark)
	ui.theme.register { name = M.DARK, type = "dark", extends = "zgx-dark", tokens = merged(tokens.dark) }
	ui.theme.register { name = M.LIGHT, type = "light", extends = "zgx-light", tokens = merged(tokens.light) }
	ui.theme.default(M.DARK)
end

export type Choice = "system" | "dark" | "light"

-- Apply Settings' theme ("system" follows Windows' app mode).
function M.apply(choice: Choice)
	if choice == "light" then
		ui.theme.set(M.LIGHT)
	elseif choice == "dark" then
		ui.theme.set(M.DARK)
	else
		ui.theme.follow_system { dark = M.DARK, light = M.LIGHT }
	end
end

-- Keep applying the theme `choice()` reads (the saved setting) as it
-- changes. The overlay doesn't follow: it scopes itself to `M.OVERLAY`.
function M.follow(choice: () -> Choice)
	local last: Choice? = nil
	ui.effect(function()
		local c = choice()
		if c ~= last then
			last = c
			M.apply(c)
		end
	end)
end

-- The caption overlay is dark always (DESIGN §9.3 Caption bar).
M.OVERLAY = M.DARK

return M
