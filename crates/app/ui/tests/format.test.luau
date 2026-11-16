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
