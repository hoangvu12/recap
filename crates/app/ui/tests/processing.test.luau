--!nonstrict
-- After Stop (screens/3a, 3b, 5): the processing card over the transcript,
-- its error and Retry, then the refined transcript with its speakers.
local fx = require("./fixtures")

local function steps(transcribing, naming, summary, error)
	local function step(id, label, status, took_ms)
		return { id = id, label = label, status = status, took_ms = took_ms or 0, error = "" }
	end
	local out = {
		step("transcribing", "Transcribing", transcribing, if transcribing == "done" then 11500 else 0),
		step("naming_speakers", "Naming speakers", naming),
		step("writing_summary", "Writing summary", summary),
	}
	for _, s in out do
		if s.status == "error" then
			s.error = error
		end
	end
	return { id = "20261116-100000", steps = out }
end

-- The Notes window just after Stop: the live rows, stopped, processing.
local function stopped(app, processing)
	app:set_state("mock_rows", fx.rows())
	app:set_state("meeting", { state = "stopped", elapsed_ms = 118000, title = "", started_ms = fx.STARTED_MS, id = "20261116-100000" })
	app:set_state("transcript", { count = 6, open = {} })
	app:set_state("processing", processing)
	app:set_state("window.notes", true)
	app:frames(2)
	return app:window("notes")
end

test("after Stop the card shows the three steps over the live transcript", function()
	local app = mount()
	local win = stopped(app, steps("running", "pending", "pending"))
	expect(win:has("#processing")).to_be(true)
	expect(win:text("#step-1-label")).to_be("Transcribing")
	expect(win:text("#step-2-label")).to_be("Naming speakers")
	expect(win:text("#step-3-label")).to_be("Writing summary")
	expect(win:has("#step-1-running")).to_be(true)
	expect(win:text("#step-1-took")).to_be("…")
	expect(win:has("#step-2-pending")).to_be(true)
	expect(win:has("#step-3-pending")).to_be(true)
	-- The live rows stay under it until the refined ones come.
	expect(win:text("#row-1-text")).to_be(fx.LINES[1][3])
	expect(win:has("#processing-error")).to_be(false)
end)

test("a done step turns into a check with the time it took", function()
	local app = mount()
	local win = stopped(app, steps("running", "pending", "pending"))
	app:set_state("processing", steps("done", "running", "pending"))
	expect(win:wait("#step-1-done")).to_be(true)
	expect(win:text("#step-1-took")).to_be("11.5 s")
	expect(win:has("#step-2-running")).to_be(true)
	expect(win:text("#step-2-took")).to_be("…")
end)

test("while it processes the footer offers Hide, and Esc hides the window", function()
	local app = mount()
	local win = stopped(app, steps("done", "running", "pending"))
	expect(win:text("#hide")).to_be("Hide")
	expect(win:has("#stop")).to_be(false)
	win:key("Escape")
	app:frames(2)
	expect(app:state("window.notes")).to_be(false)
end)

test("a failed step shows its error with Retry, which resumes it", function()
	local app = mount()
	local win = stopped(app, steps("done", "error", "pending", "Claude CLI isn't signed in."))
	expect(win:has("#step-2-error")).to_be(true)
	expect(win:text("#processing-error-text")).to_be("Claude CLI isn't signed in.")
	expect(win:text("#retry")).to_be("Retry")
	expect(win:text("#fix-provider")).to_be("Use API key…")
	expect(win:text("#retry-footer")).to_be("Retry")
	expect(win:has("#hide")).to_be(false)

	win:click("#retry")
	expect(app:state("calls")[1]).to_equal({ name = "retry_processing", arg = "20261116-100000" })
	-- Enter retries too.
	win:key("Enter")
	expect(app:state("calls")[2].name).to_be("retry_processing")
end)

test("Use API key opens the providers", function()
	local app = mount()
	local win = stopped(app, steps("error", "pending", "pending", "ElevenLabs is out of credit."))
	win:click("#fix-provider")
	app:frames(2)
	expect(app:state("settings_page")).to_be("providers")
	expect(app:state("window.settings")).to_be(true)
end)

test("once every step is done the card goes and the refined transcript shows its speakers", function()
	local app = mount()
	local win = stopped(app, steps("done", "running", "pending"))
	-- The host replaces the rows with the refined ones and lists the speakers.
	local rows = fx.rows()
	local labels = { "Speaker 1", "Speaker 2", "Speaker 3" }
	for i, row in rows do
		row.speaker = labels[(i - 1) % 3 + 1]
		row.speaker_n = (i - 1) % 3 + 1
	end
	app:set_state("mock_rows", rows)
	app:set_state("meeting", {
		state = "stopped",
		elapsed_ms = 118000,
		title = "",
		started_ms = fx.STARTED_MS,
		id = "20261116-100000",
		speakers = { { label = "Speaker 1", n = 1 }, { label = "Speaker 2", n = 2 }, { label = "Speaker 3", n = 3 } },
	})
	app:set_state("transcript", { count = 6, open = {}, generation = 2 })
	app:set_state("processing", { id = "", steps = {} })
	expect(win:wait("#processing", { gone = true })).to_be(true)
	expect(win:wait("#speaker-1", { text = "Speaker 1" })).to_be(true)
	expect(win:text("#speaker-3")).to_be("Speaker 3")
	-- The rows were read again: the refined rows' speaker chips.
	expect(win:wait("#row-1", { text = "Speaker 1" })).to_be(true)
	expect(win:text("#row-2")).to_be("Speaker 2")
	expect(win:has("#hide")).to_be(false)
end)

test("a live meeting has no card and no speakers header", function()
	local win = fx.live_meeting(mount())
	expect(win:has("#processing")).to_be(false)
	expect(win:has("#speakers")).to_be(false)
end)
