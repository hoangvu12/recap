--!strict
-- Plain logic for Home's list, no `ui`: which meetings show, how they group
-- by day, which one is selected, what a row says.
local format = require("./format")

-- A meeting as host.luau's MeetingItem.
type Item = { id: string, title: string, started_ms: number, duration_ms: number, status: string }

export type Section = { label: string, items: { Item } }

local M = {}

-- Never finished (crash, killed app): Home offers Recover.
function M.unfinished(item: Item): boolean
	return item.status ~= "done"
end

-- What the row says: its title; an unfinished meeting says when it started.
function M.title(item: Item): string
	if M.unfinished(item) then
		return `Unfinished meeting, {format.time(item.started_ms)}`
	end
	return if item.title == "" then "Untitled meeting" else item.title
end

-- The meetings whose row holds every word of `query` (any case).
function M.visible(items: { Item }, query: string): { Item }
	local words = string.split(string.lower(query), " ")
	local out = {}
	for _, item in items do
		local title = string.lower(M.title(item))
		local all = true
		for _, word in words do
			if word ~= "" and not string.find(title, word, 1, true) then
				all = false
				break
			end
		end
		if all then
			table.insert(out, item)
		end
	end
	return out
end

-- `items` (newest first) under their day: Today, Yesterday, Fri, Nov 13.
function M.sections(items: { Item }, now_ms: number): { Section }
	local out: { Section } = {}
	for _, item in items do
		local label = format.day(item.started_ms, now_ms)
		local last = out[#out]
		if last and last.label == label then
			table.insert(last.items, item)
		else
			table.insert(out, { label = label, items = { item } })
		end
	end
	return out
end

-- The selected id among `items`: `selected` if it shows, else the first.
function M.pick(items: { Item }, selected: string): string
	for _, item in items do
		if item.id == selected then
			return selected
		end
	end
	return if items[1] then items[1].id else ""
end

-- The id `delta` rows from `selected`, kept within the list.
function M.step(items: { Item }, selected: string, delta: number): string
	if #items == 0 then
		return ""
	end
	local at = 1
	for i, item in items do
		if item.id == selected then
			at = i
		end
	end
	return items[math.clamp(at + delta, 1, #items)].id
end

return M
