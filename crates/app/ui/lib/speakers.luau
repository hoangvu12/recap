--!strict
-- Naming speakers by hand: what a rename field suggests, and what the Name
-- speakers dialog changed. Pure: no ui.
local M = {}

export type Suggestion = {
	-- What the list shows: the name, or `Add “Dav”`.
	label: string,
	-- The name it gives.
	name: string,
	-- A new name (not a known person).
	add: boolean,
}

local function trim(s: string): string
	return (string.gsub(s, "^%s*(.-)%s*$", "%1"))
end
M.trim = trim

-- The known `people` matching `text` (words starting with it first, then
-- containing it), without `current`; then `Add “text”` unless someone is
-- called just that. Empty text lists everyone.
function M.suggestions(people: { string }, text: string, current: string?): { Suggestion }
	local typed = trim(text)
	local q = string.lower(typed)
	local starts, contains = {}, {}
	local exact = false
	for _, name in people do
		local lower = string.lower(name)
		if lower == q then
			exact = true
		end
		if current and lower == string.lower(current) and q ~= lower then
			continue
		end
		if q == "" then
			table.insert(starts, name)
		elseif string.find(" " .. lower, " " .. q, 1, true) then
			table.insert(starts, name)
		elseif string.find(lower, q, 1, true) then
			table.insert(contains, name)
		end
	end
	local out: { Suggestion } = {}
	for _, name in starts do
		table.insert(out, { label = name, name = name, add = false })
	end
	for _, name in contains do
		table.insert(out, { label = name, name = name, add = false })
	end
	if typed ~= "" and not exact then
		table.insert(out, { label = `Add “{typed}”`, name = typed, add = true })
	end
	return out
end

-- The names the dialog changed: speaker id -> name ("" unnames), from what
-- each field holds (`typed`); Me (locked) never.
function M.changed(speakers: { { id: string, name: string, locked: boolean } }, typed: { [string]: string }): { [string]: string }?
	local out: { [string]: string } = {}
	local any = false
	for _, s in speakers do
		local now = typed[s.id]
		if not s.locked and now ~= nil and trim(now) ~= s.name then
			out[s.id] = trim(now)
			any = true
		end
	end
	return if any then out else nil
end

return M
