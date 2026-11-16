--!strict
-- The action panel's actions on a meeting (screens/9), no `ui`: what each
-- is called, its icon and keys, which the search shows, which keys run one.
export type Action = {
	id: string,
	label: string,
	icon: string,
	keys: { string },
	-- Red (Delete).
	danger: boolean?,
}

local M = {}

M.ALL = {
	{ id = "copy_markdown", label = "Copy as Markdown", icon = "copy", keys = { "Ctrl", "Shift", "C" } },
	{ id = "export_markdown", label = "Export .md…", icon = "export", keys = { "Ctrl", "E" } },
	{ id = "name_speakers", label = "Name speakers…", icon = "speakers", keys = { "Ctrl", "R" } },
	{ id = "regenerate_summary", label = "Regenerate summary", icon = "sparkle", keys = { "Ctrl", "Shift", "R" } },
	{ id = "show_in_folder", label = "Show recording in folder", icon = "folder", keys = { "Ctrl", "O" } },
	{ id = "delete_meeting", label = "Delete meeting", icon = "trash", keys = { "Ctrl", "Shift", "⌫" }, danger = true },
} :: { Action }

-- The actions whose label holds every word of `query` (any case).
function M.shown(query: string): { Action }
	local words = string.split(string.lower(query), " ")
	local out = {}
	for _, action in M.ALL do
		local label = string.lower(action.label)
		local all = true
		for _, word in words do
			if word ~= "" and not string.find(label, word, 1, true) then
				all = false
				break
			end
		end
		if all then
			table.insert(out, action)
		end
	end
	return out
end

-- The selected action's id among those `query` shows: `selected` if shown,
-- else the first ("" when none shows).
function M.pick(query: string, selected: string): string
	local shown = M.shown(query)
	for _, action in shown do
		if action.id == selected then
			return selected
		end
	end
	return if shown[1] then shown[1].id else ""
end

-- The id `delta` rows from `selected` among those shown, kept within them.
function M.step(query: string, selected: string, delta: number): string
	local shown = M.shown(query)
	if #shown == 0 then
		return ""
	end
	local at = 1
	for i, action in shown do
		if action.id == M.pick(query, selected) then
			at = i
		end
	end
	return shown[math.clamp(at + delta, 1, #shown)].id
end

-- The action key event `e` runs, if any (Ctrl Shift C, Ctrl E…).
function M.for_key(e: { key: string, ctrl: boolean?, shift: boolean?, alt: boolean? }): string?
	if not e.ctrl or e.alt then
		return nil
	end
	local key = if #e.key == 1 then string.upper(e.key) else e.key
	if e.shift then
		if key == "C" then
			return "copy_markdown"
		elseif key == "R" then
			return "regenerate_summary"
		elseif key == "Backspace" then
			return "delete_meeting"
		end
		return nil
	end
	if key == "E" then
		return "export_markdown"
	elseif key == "R" then
		return "name_speakers"
	elseif key == "O" then
		return "show_in_folder"
	end
	return nil
end

return M
