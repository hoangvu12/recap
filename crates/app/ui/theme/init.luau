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

return M
