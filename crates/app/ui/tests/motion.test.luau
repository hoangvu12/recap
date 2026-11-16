--!nonstrict
-- Motion (motion_spec §2, DESIGN §9.4) and the theme: what moves, what
-- reduced motion stills, and the theme Settings picks (the overlay stays
-- dark). A node fading in from 0 is "hidden" in the tree until it shows.
local fx = require("./fixtures")
local format = require("../lib/format")

-- Whether `target`'s own node doesn't show yet (transparent).
local function hidden(win, target): boolean
	local tree = win:tree(target)
	local first = string.match(tree, "^[^\n]*") or ""
	return string.find(first, " hidden", 1, true) ~= nil
end

local function reduce(app)
	app:set_state("reduce_motion", true)
end

-- The test meeting stored, its summary on the Summary tab.
local function summary(app)
	app:set_state("mock_rows", fx.named_rows())
	app:set_state("meeting", { state = "stopped", elapsed_ms = 118000, title = "Mobile app launch on Dec 1", started_ms = fx.STARTED_MS, id = "20261116-100000" })
	app:set_state("transcript", { count = 3, open = {} })
	app:set_state("summary", fx.summary())
	app:set_state("window.notes", true)
	app:frames(2)
	local win = app:window("notes")
	win:click("#tab-summary")
	app:frames(1)
	return win
end

-- Theme ---------------------------------------------------------------------

local function settings(theme)
	return {
		my_name = "",
		theme = theme,
		autostart = false,
		caption_my_speech = false,
		caption_size = 20,
		caption_lines = 2,
		partial_text = true,
		hide_from_share = true,
		microphone = "",
		meeting_audio = "system",
		suggest_on_call = true,
		glossary = {},
		hotkeys = { start_notes = "Ctrl+Alt+N", pause = "Ctrl+Alt+P", click_through = "Ctrl+Alt+C", captions = "Ctrl+Alt+O" },
		storage_folder = "",
		keep_audio_days = 0,
	}
end

test("the theme is the one Settings picks: light is a token swap", function()
	local app = mount()
	app:set_state("settings", settings("light"))
	app:frames(1)
	expect(app:text("#theme")).to_be("recap-light")
	app:set_state("settings", settings("dark"))
	app:frames(1)
	expect(app:text("#theme")).to_be("recap-dark")
end)

test("the overlay stays dark whatever the theme", function()
	local app = mount()
	app:set_state("settings", settings("light"))
	app:set_state("window.overlay", true)
	app:frames(2)
	local win = app:window("overlay")
	expect(win:text("#overlay-theme")).to_be("recap-dark")
end)

-- Summary reveal -------------------------------------------------------------

test("summary sections rise in one after another", function()
	local app = mount()
	local win = summary(app)
	-- The first is on its way; the fifth starts 160 ms in.
	app:frames(8)
	expect(hidden(win, "#summary-overview")).to_be(false)
	expect(hidden(win, "#summary-decisions")).to_be(true)
	app:frames(30)
	expect(hidden(win, "#summary-decisions")).to_be(false)
end)

test("reduced motion: the summary is one 100 ms fade, no stagger", function()
	local app = mount()
	reduce(app)
	local win = summary(app)
	app:frames(8)
	expect(hidden(win, "#summary-overview")).to_be(false)
	expect(hidden(win, "#summary-decisions")).to_be(false)
end)

-- Loops ------------------------------------------------------------------------

test("reduced motion: the rec dot is solid, and follows the setting live", function()
	local app = mount()
	local win = fx.live_meeting(app)
	expect(win:has("#rec-dot")).to_be(true)
	expect(win:has("#rec-dot-solid")).to_be(false)
	reduce(app)
	app:frames(1)
	expect(win:has("#rec-dot")).to_be(false)
	expect(win:has("#rec-dot-solid")).to_be(true)
	app:set_state("reduce_motion", false)
	app:frames(1)
	expect(win:has("#rec-dot")).to_be(true)
end)

test("reduced motion: the processing spinner stands still", function()
	local app = mount()
	app:set_state("mock_rows", fx.named_rows())
	app:set_state("meeting", { state = "stopped", elapsed_ms = 118000, title = "", started_ms = fx.STARTED_MS, id = "m" })
	app:set_state("transcript", { count = 3, open = {} })
	app:set_state("processing", {
		id = "m",
		steps = { { id = "transcribing", label = "Transcribing", status = "running", took_ms = 0, error = "" } },
	})
	app:set_state("window.notes", true)
	app:frames(2)
	local win = app:window("notes")
	expect(win:has("#step-1-running")).to_be(true)
	expect(win:has("#step-1-spin")).to_be(true)
	reduce(app)
	app:frames(1)
	expect(win:has("#step-1-running")).to_be(true)
	expect(win:has("#step-1-spin")).to_be(false)
end)

test("all steps done: the card holds 400 ms on the checks, then fades out", function()
	local app = mount()
	local function steps(last)
		local function step(id, label, status)
			return { id = id, label = label, status = status, took_ms = 1000, error = "" }
		end
		return {
			id = "m",
			steps = {
				step("transcribing", "Transcribing", "done"),
				step("naming_speakers", "Naming speakers", "done"),
				step("writing_summary", "Writing summary", last),
			},
		}
	end
	app:set_state("mock_rows", fx.named_rows())
	app:set_state("meeting", { state = "stopped", elapsed_ms = 118000, title = "", started_ms = fx.STARTED_MS, id = "m" })
	app:set_state("transcript", { count = 3, open = {} })
	app:set_state("processing", steps("running"))
	app:set_state("window.notes", true)
	app:frames(2)
	local win = app:window("notes")
	app:set_state("processing", steps("done"))
	app:frames(18)
	expect(win:has("#step-3-done")).to_be(true)
	expect(hidden(win, "#processing")).to_be(false)
	expect(win:wait("#processing", { gone = true })).to_be(true)
end)

-- Live transcript --------------------------------------------------------------

-- The live meeting at its end; then row 7 turns final and `n` new rows come.
local function arrive(app, n)
	local win = fx.live_meeting(app)
	win:scroll("#transcript", 100000)
	app:frames(3)
	local rows = fx.rows()
	rows[7] = table.clone(fx.OPEN)
	rows[7].final = rows[7].partial
	rows[7].partial = ""
	rows[7].open = false
	local open = {}
	for i = 8, 7 + n do
		local row = { index = i, source = "them", start_ms = 60000 + i * 1000, final = "", partial = `Words of row {i}`, open = true }
		rows[i] = row
		open[tostring(i)] = row
	end
	app:set_state("mock_rows", rows)
	app:set_state("transcript", { count = 7 + n, open = open })
	app:frames(1)
	return win
end

test("partial words turning final keep the paragraph as it reads", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:scroll("#transcript", 100000)
	app:frames(2)
	local closed = table.clone(fx.OPEN)
	closed.final, closed.partial, closed.open = fx.OPEN.partial, "", false
	app:set_state("transcript", { count = 7, open = {}, revised = { ["7"] = closed } })
	app:frames(1)
	-- Mid colour ease, then merged: the same words either way.
	expect(win:text("#row-7-text")).to_be(fx.OPEN.partial)
	app:frames(12)
	expect(win:text("#row-7-text")).to_be(fx.OPEN.partial)
end)

test("a new row fades in and rises 4 px", function()
	local app = mount()
	local win = arrive(app, 1)
	expect(hidden(win, "#row-8-body")).to_be(true)
	app:frames(15)
	expect(hidden(win, "#row-8-body")).to_be(false)
end)

test("rows already there when the window opens don't animate", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:scroll("#transcript", 100000)
	app:frames(1)
	expect(hidden(win, "#row-7-body")).to_be(false)
end)

test("more than 3 rows at once just appear", function()
	local app = mount()
	local win = arrive(app, 4)
	win:scroll("#transcript", 100000)
	app:frames(1)
	expect(hidden(win, "#row-11-body")).to_be(false)
end)

test("a pause marker fades in over 150 ms", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:scroll("#transcript", 100000)
	app:frames(3)
	local rows = fx.rows()
	rows[7] = table.clone(fx.OPEN)
	rows[7].final, rows[7].partial, rows[7].open = rows[7].partial, "", false
	local paused = format.local_ms({ year = 2026, month = 11, day = 16, hour = 15, min = 54 })
	rows[8] = {
		index = 8,
		source = "them",
		start_ms = 70000,
		final = "",
		partial = "",
		open = false,
		pause = { paused_ms = paused, resumed_ms = paused + 240000 },
	}
	app:set_state("mock_rows", rows)
	app:set_state("transcript", { count = 8, open = {} })
	app:frames(1)
	expect(hidden(win, "#row-8")).to_be(true)
	app:frames(12)
	expect(hidden(win, "#row-8")).to_be(false)
end)

test("reduced motion: a new row only fades, in 100 ms", function()
	local app = mount()
	reduce(app)
	local win = arrive(app, 1)
	app:frames(7)
	expect(hidden(win, "#row-8-body")).to_be(false)
end)
