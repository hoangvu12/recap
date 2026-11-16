--!strict
-- Which live transcript rows are arriving now (motion_spec §2.3): rows of a
-- live meeting that came in a small batch (3 or fewer at once) while the
-- same transcript was showing. A row mounting while it is arriving plays its entrance; one
-- mounted later (scrolled back to), or one of a catch-up batch, or a row of a
-- transcript just opened, appears as it is.
local host = require("@app/host")

local M = {}

-- More than this many rows in one change: a catch-up, no entrances.
M.BATCH = 3
-- How long a row counts as arriving (Wispr's "arriving" window).
M.WINDOW_MS = 600

-- Row index -> until when it arrives (os.clock seconds). A deadline, not a
-- timer: a timer started in the effect below would be stopped by its next
-- run, and live partials run it many times a second.
local arriving: { [number]: number } = {}
local last_count = 0
local last_generation = -1

local function mark(indexes: { number })
	local until_s = os.clock() + M.WINDOW_MS / 1000
	for _, i in indexes do
		arriving[i] = until_s
	end
end

ui.effect(function()
	local t = host.transcript:get()
	local generation = t.generation or 0
	local count = t.count
	-- Only a live meeting's rows arrive; a stored one's are just there.
	local s = host.meeting:peek().state
	local live = s == "recording" or s == "paused"
	if live and generation == last_generation and count > last_count and count - last_count <= M.BATCH then
		local new = {}
		for i = last_count + 1, count do
			table.insert(new, i)
		end
		mark(new)
	elseif generation ~= last_generation or count < last_count then
		table.clear(arriving)
	end
	last_generation, last_count = generation, count
end)

-- Is row `index` (from 1) arriving now?
function M.arriving(index: number): boolean
	local until_s = arriving[index]
	if until_s == nil then
		return false
	end
	if os.clock() >= until_s then
		arriving[index] = nil
		return false
	end
	return true
end

return M
