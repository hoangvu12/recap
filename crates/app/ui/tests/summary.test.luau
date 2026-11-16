--!nonstrict
-- The Summary tab (screens/4): overview, topics, next steps to tick,
-- decisions; it opens when processing writes the summary; without one it
-- offers to write it (Regenerate).
local fx = require("./fixtures")

local ID = "20261116-100000"

local function stored(app, summary, processing)
	app:set_state("mock_rows", fx.named_rows())
	app:set_state("meeting", { state = "stopped", elapsed_ms = 118000, title = "Mobile app launch on Dec 1", started_ms = fx.STARTED_MS, id = ID })
	app:set_state("transcript", { count = 3, open = {} })
	app:set_state("processing", processing or { id = "", steps = {} })
	app:set_state("summary", summary)
	app:set_state("window.notes", true)
	app:frames(2)
	return app:window("notes")
end

local function none()
	return { id = "", overview = "", topics = {}, next_steps = {}, decisions = {} }
end

local function steps(naming, summary)
	local function step(id, label, status)
		return { id = id, label = label, status = status, took_ms = 0, error = "" }
	end
	return {
		id = ID,
		steps = {
			step("transcribing", "Transcribing", "done"),
			step("naming_speakers", "Naming speakers", naming),
			step("writing_summary", "Writing summary", summary),
		},
	}
end

test("the summary reads as Wispr's: overview, topics, next steps, decisions", function()
	local app = mount()
	local win = stored(app, fx.summary())
	win:click("#tab-summary")
	app:frames(2)
	expect(win:text("#summary-overview-text")).to_be(fx.OVERVIEW)
	expect(win:text("#topic-1-heading")).to_be("Launch readiness")
	expect(win:text("#topic-1-bullet-2")).to_be("Backend: payments API ready, but slow above 2,000 users. Caching needs one more week.")
	expect(win:text("#topic-2-heading")).to_be("Release notes")
	expect(win:text("#next-1-text")).to_be("Add caching and rerun the load test")
	expect(win:text("#next-1-owner")).to_be("David")
	expect(win:text("#next-1-due")).to_be("Before Dec 1")
	expect(win:text("#next-5-owner")).to_be("Tom")
	-- Nobody's and undated: no chip, no date.
	expect(win:has("#next-6-owner")).to_be(false)
	expect(win:has("#next-6-due")).to_be(false)
	expect(win:text("#decision-2")).to_be("If load testing still fails: soft launch in India only.")
	expect(win:has("#summary-empty")).to_be(false)
end)

test("a next step ticks on and off", function()
	local app = mount()
	local win = stored(app, fx.summary())
	win:click("#tab-summary")
	app:frames(2)
	-- Ticked ones show the check.
	expect(win:has("#next-3-checked")).to_be(true)
	expect(win:has("#next-1-checked")).to_be(false)

	-- Every section has faded in within 500 ms (30 frames).
	app:frames(30)
	win:click("#next-1")
	expect(app:state("calls")[1]).to_equal({ name = "tick_next_step", arg = { id = ID, index = 1, done = true } })
	win:click("#next-3")
	expect(app:state("calls")[2]).to_equal({ name = "tick_next_step", arg = { id = ID, index = 3, done = false } })

	-- The host keeps it and says so: the box follows.
	local s = fx.summary()
	s.next_steps[1].done = true
	app:set_state("summary", s)
	expect(win:wait("#next-1-checked")).to_be(true)
	expect(win:text("#next-1-text")).to_be("Add caching and rerun the load test")
end)

test("the summary written while processing opens the Summary tab", function()
	local app = mount()
	local win = stored(app, none(), steps("done", "running"))
	expect(app:state("notes_tab")).to_be("transcript")
	app:set_state("summary", fx.summary())
	app:frames(2)
	expect(app:state("notes_tab")).to_be("summary")
	expect(win:wait("#summary-overview-text")).to_be(true)
end)

test("while it processes the Summary tab shows the card", function()
	local app = mount()
	local win = stored(app, none(), steps("running", "pending"))
	win:click("#tab-summary")
	app:frames(2)
	expect(win:has("#processing")).to_be(true)
	expect(win:has("#summary-empty")).to_be(false)
end)

test("a meeting without a summary offers to write one", function()
	local app = mount()
	local win = stored(app, none())
	win:click("#tab-summary")
	app:frames(2)
	expect(win:has("#summary-empty")).to_be(true)
	win:click("#regenerate")
	expect(app:state("calls")[1]).to_equal({ name = "regenerate_summary", arg = ID })
end)

test("opening a stored meeting keeps the tab the user is on", function()
	local app = mount()
	stored(app, fx.summary())
	expect(app:state("notes_tab")).to_be("transcript")
end)
