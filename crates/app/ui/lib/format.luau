--!strict
-- Plain formatting, no `ui`: times and dates as the screens show them.
local M = {}

-- Meeting time: `01:02`, `41:12`, past an hour `1:02:03`.
function M.clock(ms: number): string
	local total = math.floor(math.max(ms, 0) / 1000)
	local h = total // 3600
	local m = (total % 3600) // 60
	local s = total % 60
	if h > 0 then
		return string.format("%d:%02d:%02d", h, m, s)
	end
	return string.format("%02d:%02d", m, s)
end

local DAYS = { "Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat" }
local MONTHS = { "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec" }

-- A local date and time: `Mon, Nov 16, 10:00 AM`.
function M.date(epoch_ms: number): string
	local t = os.date("*t", math.floor(epoch_ms / 1000)) :: any
	local hour = t.hour % 12
	if hour == 0 then
		hour = 12
	end
	local half = if t.hour < 12 then "AM" else "PM"
	return string.format("%s, %s %d, %d:%02d %s", DAYS[t.wday], MONTHS[t.month], t.day, hour, t.min, half)
end

return M
