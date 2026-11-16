--!nonstrict
-- The Notes window (screens/2 "Notes live") from a mocked host state.
local fx = require("./fixtures")
local format = require("../lib/format")

test("the Notes window opens when the host says so", function()
	local app = mount()
	expect(app:window("notes")).to_be_nil()
	app:set_state("window.notes", true)
	app:frames(2)
	expect(app:window("notes")).never.to_be_nil()
end)

test("live rows show the side's chip, the time and the text", function()
	local win = fx.live_meeting(mount())
	expect(win:text("#row-1")).to_be("Me")
	expect(win:text("#row-1-time")).to_be("00:00")
	expect(win:text("#row-1-text")).to_be(fx.LINES[1][3])
	expect(win:text("#row-2")).to_be("Them")
	expect(win:text("#row-2-time")).to_be("00:11")
	expect(win:text("#row-2-text")).to_be(fx.LINES[2][3])
end)

test("the newest row shows its partial words", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:scroll("#transcript", 100000)
	app:frames(2)
	expect(win:wait("#row-7-text", { text = fx.OPEN.partial })).to_be(true)
	expect(win:text("#row-7-time")).to_be("01:02")
end)

test("a partial that turns final keeps its words, final after final", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:scroll("#transcript", 100000)
	app:frames(2)
	-- The row's side speaks on: final words, then a new partial.
	local open = table.clone(fx.OPEN)
	open.final = "Okay, that works for me."
	open.partial = "Let's make"
	app:set_state("transcript", { count = 7, open = { ["7"] = open } })
	expect(win:wait("#row-7-text", { text = "Okay, that works for me. Let's make" })).to_be(true)
end)

test("the header has the title, the date and the tabs", function()
	local win = fx.live_meeting(mount())
	expect(win:text("#meeting-title")).to_be("New meeting")
	expect(string.match(win:text("#meeting-date"), "^%a%a%a, %a%a%a %d+, %d+:%d%d [AP]M$")).never.to_be_nil()
	expect(win:has("#tab-notes")).to_be(true)
	expect(win:has("#tab-transcript")).to_be(true)
	expect(win:has("#tab-summary")).to_be(true)
end)

test("the footer shows the timer, Stop and Actions", function()
	local win = fx.live_meeting(mount())
	expect(win:text("#timer")).to_be("01:02")
	expect(win:text("#stop")).to_be("Stop")
	expect(win:text("#actions")).to_be("Actions")
	expect(win:text("#actions-keys")).to_be("Ctrl")
end)

test("Stop asks the host to stop", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:click("#stop")
	expect(app:state("calls")).to_equal({ { name = "stop", arg = nil } })
end)

test("Enter stops too", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:key("Enter")
	expect(#app:state("calls")).to_be(1)
	expect(app:state("calls")[1].name).to_be("stop")
end)

test("Stop is gone once the meeting stopped", function()
	local app = mount()
	local win = fx.live_meeting(app)
	app:set_state("meeting", { state = "stopped", elapsed_ms = 62000, title = "", started_ms = fx.STARTED_MS })
	expect(win:wait("#stop", { gone = true })).to_be(true)
end)

test("the Notes tab is where my notes go", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:click("#tab-notes")
	win:type("#my-notes", "Ask about the India fallback")
	expect(app:state("my_notes")).to_be("Ask about the India fallback")
end)

test("Stop keeps my notes with the meeting first: they steer the summary", function()
	local app = mount()
	fx.live_meeting(app)
	app:set_state("meeting", { state = "recording", elapsed_ms = 62000, title = "", started_ms = fx.STARTED_MS, id = "20261116-100000" })
	app:set_state("my_notes", "Ask about the India fallback")
	local win = app:window("notes")
	win:click("#stop")
	expect(app:state("calls")).to_equal({
		{ name = "set_my_notes", arg = { id = "20261116-100000", text = "Ask about the India fallback" } },
		{ name = "stop", arg = nil },
	})
end)

-- A long live transcript: `n` final rows, alternating sides.
local function long_meeting(app, n)
	local rows = {}
	for i = 1, n do
		local line = fx.LINES[(i - 1) % #fx.LINES + 1]
		rows[i] = { index = i, source = line[1], start_ms = i * 5000, final = line[3], partial = "", open = false }
	end
	app:set_state("mock_rows", rows)
	app:set_state("meeting", { state = "recording", elapsed_ms = n * 5000, title = "", started_ms = fx.STARTED_MS })
	app:set_state("transcript", { count = n, open = {} })
	app:set_state("window.notes", true)
	app:frames(3)
	local win = app:window("notes")
	-- A meeting's transcript grows from empty, at its end; this one starts
	-- with rows, so go there.
	win:scroll("#transcript", 100000)
	app:frames(3)
	return win
end

-- `n` more final rows arrive.
local function more_rows(app, n)
	local rows = table.clone(app:state("mock_rows"))
	for _ = 1, n do
		local i = #rows + 1
		rows[i] = { index = i, source = "them", start_ms = i * 5000, final = "And one more thing.", partial = "", open = false }
	end
	app:set_state("mock_rows", rows)
	app:set_state("transcript", { count = #rows, open = {} })
	app:frames(2)
end

test("the transcript follows the bottom until scrolled up, then the pill takes you back", function()
	local app = mount()
	local win = long_meeting(app, 30)
	-- Pinned: it follows new rows, no pill.
	expect(app:state("transcript_at_end")).to_be(true)
	more_rows(app, 2)
	expect(app:state("transcript_at_end")).to_be(true)
	-- (gone, or fading out if it showed as the window opened)
	expect(win:wait("#jump-to-live", { gone = true })).to_be(true)
	expect(app:state("seen_count")).to_be(32)

	-- Scrolled up: it stops following and the pill says Live.
	win:scroll("#transcript", -600)
	app:frames(2)
	expect(app:state("transcript_at_end")).to_be(false)
	expect(win:wait("#jump-to-live", { text = "Live" })).to_be(true)
	expect(win:has("#jump-dot")).to_be(false)

	-- New rows while away: "N new" with the accent dot; the view stays put.
	more_rows(app, 3)
	expect(app:state("transcript_at_end")).to_be(false)
	expect(win:wait("#jump-to-live", { text = "3 new" })).to_be(true)
	expect(win:has("#jump-dot")).to_be(true)

	-- The pill (in once its 150 ms fade is done): back to the end,
	-- following again, the pill gone.
	app:frames(10)
	win:click("#jump-to-live")
	app:frames(20)
	expect(app:state("seen_count")).to_be(35)
	expect(win:wait("#jump-to-live", { gone = true })).to_be(true)
	expect(app:state("transcript_at_end")).to_be(true)
end)

test("a pause marker row shows the pause, from and to", function()
	local app = mount()
	local paused = format.local_ms({ year = 2026, month = 11, day = 16, hour = 15, min = 54 })
	local resumed = format.local_ms({ year = 2026, month = 11, day = 16, hour = 15, min = 58 })
	local rows = fx.rows()
	rows[7] = {
		index = 7,
		source = "them",
		start_ms = 62000,
		final = "",
		partial = "",
		open = false,
		pause = { paused_ms = paused, resumed_ms = resumed },
	}
	app:set_state("mock_rows", rows)
	app:set_state("meeting", { state = "recording", elapsed_ms = 62000, title = "", started_ms = fx.STARTED_MS })
	app:set_state("transcript", { count = 7, open = {} })
	app:set_state("window.notes", true)
	app:frames(3)
	local win = app:window("notes")
	win:scroll("#transcript", 100000)
	app:frames(2)
	expect(win:text("#row-7-pause")).to_be("15:54 – 15:58")
	expect(win:has("#row-7-text")).to_be(false)
	expect(win:has("#row-7-time")).to_be(false)
end)

test("a row whose words were taken back (an echo) shows nothing", function()
	local app = mount()
	local win = fx.live_meeting(app)
	expect(win:text("#row-1-text")).to_be(fx.LINES[1][3])
	-- Row 1 was read as final; the host sends it again, revised.
	local blank = table.clone(fx.rows()[1])
	blank.final = ""
	app:set_state("transcript", { count = 7, open = { ["7"] = fx.OPEN }, revised = { ["1"] = blank } })
	expect(win:wait("#row-1-text", { gone = true })).to_be(true)
	expect(win:has("#row-1-time")).to_be(false)
	expect(win:text("#row-2-text")).to_be(fx.LINES[2][3])
end)

test("recording: the rec dot breathes and the timer runs", function()
	local app = mount()
	local win = fx.live_meeting(app)
	expect(win:has("#rec-dot")).to_be(true)
	expect(win:has("#rec-dot-still")).to_be(false)
	expect(win:text("#timer")).to_be("01:02")
	app:set_state("meeting", { state = "recording", elapsed_ms = 63000, title = "", started_ms = fx.STARTED_MS })
	expect(win:wait("#timer", { text = "01:03" })).to_be(true)
end)

test("paused: the dot stills, the timer says Paused, Resume is the primary action", function()
	local app = mount()
	local win = fx.live_meeting(app)
	app:set_state("meeting", { state = "paused", elapsed_ms = 62000, title = "", started_ms = fx.STARTED_MS })
	app:frames(2)
	expect(win:has("#rec-dot")).to_be(false)
	expect(win:has("#rec-dot-still")).to_be(true)
	expect(win:text("#paused")).to_be("Paused")
	expect(win:text("#resume")).to_be("Resume")
	-- Enter resumes; Stop is still there.
	win:key("Enter")
	expect(app:state("calls")[1].name).to_be("resume")
	win:click("#stop")
	expect(app:state("calls")[2].name).to_be("stop")
end)

test("a notice from the host shows above the footer", function()
	local app = mount()
	local win = fx.live_meeting(app)
	expect(win:has("#notice")).to_be(false)
	app:set_state("meeting", {
		state = "recording",
		elapsed_ms = 62000,
		title = "",
		started_ms = fx.STARTED_MS,
		notice = "Live captions are off: ElevenLabs has no API key",
	})
	expect(win:wait("#notice", { text = "Live captions are off: ElevenLabs has no API key" })).to_be(true)
end)

test("scrolled away from the end, a pill counts the new rows", function()
	local app = mount()
	local win = fx.live_meeting(app)
	expect(win:has("#jump-to-live")).to_be(false)
	app:set_state("transcript_at_end", false)
	app:set_state("seen_count", 4)
	expect(win:wait("#jump-to-live", { text = "3 new" })).to_be(true)
	app:frames(10)
	win:click("#jump-to-live")
	expect(app:state("seen_count")).to_be(7)
end)
