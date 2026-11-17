--!strict
-- A shortcut's keycaps as a screen reader should say them. zgx has no node
-- prop for UI Automation's AcceleratorKey (only a menu item's `shortcut`),
-- so buttons put this in their `description` (ADR 0023).
local M = {}

local SPOKEN: { [string]: string } = {
	["↵"] = "Enter",
	["Esc"] = "Escape",
	["↑"] = "Up",
	["↓"] = "Down",
	["⌫"] = "Backspace",
}

function M.spoken(caps: { string }): string
	local out = {}
	for _, cap in caps do
		table.insert(out, SPOKEN[cap] or cap)
	end
	return table.concat(out, "+")
end

return M
