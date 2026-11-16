--!nonstrict
local format = require("../lib/format")

test("meeting time reads as the screens show it", function()
	expect(format.clock(0)).to_be("00:00")
	expect(format.clock(62000)).to_be("01:02")
	expect(format.clock(41 * 60000 + 12000)).to_be("41:12")
	expect(format.clock(3600000 + 2 * 60000 + 3000)).to_be("1:02:03")
end)

test("a date reads Mon, Nov 16, 10:00 AM", function()
	local d = format.date(1794823200000)
	expect(string.match(d, "^%a%a%a, %a%a%a %d+, %d+:%d%d [AP]M$")).never.to_be_nil()
end)

test("a duration reads as Home shows it", function()
	expect(format.duration(0)).to_be("0:00")
	expect(format.duration(118000)).to_be("1:58")
	expect(format.duration(32 * 60000 + 10000)).to_be("32:10")
	expect(format.duration(3600000 + 2 * 60000 + 3000)).to_be("1:02:03")
end)

local function local_ms(y, m, d, h, min)
	return format.local_ms({ year = y, month = m, day = d, hour = h, min = min })
end

test("local times read back as given", function()
	local ms = local_ms(2026, 11, 16, 10, 0)
	local t = os.date("*t", ms // 1000)
	expect(t.hour).to_be(10)
	expect(t.day).to_be(16)
end)

test("days read Today, Yesterday, then the date", function()
	local now = local_ms(2026, 11, 16, 9, 30)
	expect(format.day(local_ms(2026, 11, 16, 0, 5), now)).to_be("Today")
	expect(format.day(local_ms(2026, 11, 15, 23, 59), now)).to_be("Yesterday")
	expect(format.day(local_ms(2026, 11, 13, 10, 0), now)).to_be("Fri, Nov 13")
	-- Across a month and a year.
	expect(format.day(local_ms(2026, 12, 31, 18, 0), local_ms(2027, 1, 1, 8, 0))).to_be("Yesterday")
end)

test("a pause reads 15:54 – 15:58", function()
	expect(format.span(local_ms(2026, 11, 15, 15, 54), local_ms(2026, 11, 15, 15, 58))).to_be("15:54 – 15:58")
	expect(format.span(local_ms(2026, 11, 15, 9, 5), local_ms(2026, 11, 15, 10, 0))).to_be("09:05 – 10:00")
end)

test("a time of day reads 4:12 PM", function()
	expect(format.time(local_ms(2026, 11, 15, 16, 12))).to_be("4:12 PM")
	expect(format.time(local_ms(2026, 11, 15, 0, 5))).to_be("12:05 AM")
end)
