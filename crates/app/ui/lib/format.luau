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

-- A meeting's length in a list: `1:58`, `32:10`, `1:02:03`.
function M.duration(ms: number): string
	local total = math.floor(math.max(ms, 0) / 1000)
	local h = total // 3600
	local m = (total % 3600) // 60
	local s = total % 60
	if h > 0 then
		return string.format("%d:%02d:%02d", h, m, s)
	end
	return string.format("%d:%02d", m, s)
end

local DAYS = { "Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat" }
local MONTHS = { "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec" }

local function local_time(epoch_ms: number): any
	return os.date("*t", math.floor(epoch_ms / 1000))
end

-- A local time of day: `4:12 PM`.
function M.time(epoch_ms: number): string
	local t = local_time(epoch_ms)
	local hour = t.hour % 12
	if hour == 0 then
		hour = 12
	end
	return string.format("%d:%02d %s", hour, t.min, if t.hour < 12 then "AM" else "PM")
end

-- A local date and time: `Mon, Nov 16, 10:00 AM`.
function M.date(epoch_ms: number): string
	local t = local_time(epoch_ms)
	return string.format("%s, %s %d, %s", DAYS[t.wday], MONTHS[t.month], t.day, M.time(epoch_ms))
end

-- How far local time is ahead of UTC at `secs`, in seconds.
local function utc_offset(secs: number): number
	return os.time(os.date("*t", secs) :: any) - os.time(os.date("!*t", secs) :: any)
end

-- The local day `epoch_ms` falls on, counted from 1970.
local function day_number(epoch_ms: number): number
	local secs = math.floor(epoch_ms / 1000)
	return (secs + utc_offset(secs)) // 86400
end

-- Local time `fields` (year, month, day, hour, min) in ms. Luau's
-- `os.time(table)` reads a table as UTC, not local time.
function M.local_ms(fields: { year: number, month: number, day: number, hour: number?, min: number? }): number
	local secs = os.time({
		year = fields.year,
		month = fields.month,
		day = fields.day,
		hour = fields.hour or 12,
		min = fields.min or 0,
		sec = 0,
	})
	return (secs - utc_offset(secs)) * 1000
end

-- The local day `epoch_ms` falls on, seen from `now_ms`: `Today`,
-- `Yesterday`, else `Fri, Nov 13`.
function M.day(epoch_ms: number, now_ms: number): string
	local back = day_number(now_ms) - day_number(epoch_ms)
	if back == 0 then
		return "Today"
	elseif back == 1 then
		return "Yesterday"
	end
	local t = local_time(epoch_ms)
	return string.format("%s, %s %d", DAYS[t.wday], MONTHS[t.month], t.day)
end

return M
