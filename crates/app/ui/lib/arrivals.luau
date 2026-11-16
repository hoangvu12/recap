--!strict
-- Which live transcript rows are arriving now (motion_spec §2.3): rows that
-- came in a small batch (3 or fewer at once) while the same transcript was
-- showing. A row mounting while it is arriving plays its entrance; one
-- mounted later (scrolled back to), or one of a catch-up batch, or a row of a
-- transcript just opened, appears as it is.
local host = require("@app/host")

local M = {}

-- More than this many rows in one change: a catch-up, no entrances.
M.BATCH = 3
-- How long a row counts as arriving (Wispr's "arriving" window).
M.WINDOW_MS = 600

local arriving: { [number]: boolean } = {}
local last_count = 0
local last_generation = -1

local function forget(indexes: { number })
	ui.after(M.WINDOW_MS, function()
		for _, i in indexes do
			arriving[i] = nil
		end
	end)
end

ui.effect(function()
	local t = host.transcript:get()
	local generation = t.generation or 0
	local count = t.count
	if generation == last_generation and count > last_count and count - last_count <= M.BATCH then
		local new = {}
		for i = last_count + 1, count do
			arriving[i] = true
			table.insert(new, i)
		end
		forget(new)
	elseif generation ~= last_generation or count < last_count then
		table.clear(arriving)
	end
	last_generation, last_count = generation, count
end)

-- Is row `index` (from 1) arriving now?
function M.arriving(index: number): boolean
	return arriving[index] == true
end

return M
