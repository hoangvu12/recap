--!nonstrict
-- Speakers and playback after processing: a line plays from where it
-- starts, a chip renames inline (screens/5), the Name speakers dialog with
-- quotes and clips (screens/6), and the summary written again with only
-- what changed washed (motion_spec §2.7).
local fx = require("./fixtures")
local speakers = require("../lib/speakers")

local ID = "20261116-100000"

local function stored(app)
	app:set_state("mock_rows", fx.named_rows())
	app:set_state("mock_people", { "David", "Priya", "Tom" })
	app:set_state("meeting", {
		state = "stopped",
		elapsed_ms = 118000,
		title = "Mobile app launch on Dec 1",
		started_ms = fx.STARTED_MS,
		id = ID,
		speakers = { { label = "Tom", n = 1 }, { label = "Priya", n = 2 }, { label = "David", n = 3 } },
	})
	app:set_state("transcript", { count = 3, open = {} })
	app:set_state("window.notes", true)
	app:frames(2)
	return app:window("notes")
end

local function calls(app, name)
	local out = {}
	for _, call in app:state("calls") do
		if call.name == name then
			table.insert(out, call.arg)
		end
	end
	return out
end

-- Playback -----------------------------------------------------------------

test("a click on a line plays the meeting from there; again stops it", function()
	local app = mount()
	local win = stored(app)
	win:click("#row-2-text")
	expect(app:state("calls")[1]).to_equal({ name = "play", arg = { id = ID, key = "line-2", from_ms = 11000 } })
	expect(win:has("#row-2-playing")).to_be(false)

	-- The host says it plays: the row shows it, with a pause to stop.
	app:set_state("playback", { key = "line-2", duration_ms = 107000 })
	expect(win:wait("#row-2-playing")).to_be(true)
	expect(win:has("#row-1-playing")).to_be(false)
	win:click("#row-2-text")
	expect(app:state("calls")[2]).to_equal({ name = "stop_playback", arg = nil })
end)

test("Play from here ↵ plays from the line last clicked", function()
	local app = mount()
	local win = stored(app)
	expect(win:has("#play-from-here")).to_be(false)
	win:click("#row-3-text")
	app:set_state("playback", { key = "", duration_ms = 0 })
	expect(win:wait("#play-from-here", { text = "Play from here" })).to_be(true)
	win:key("Enter")
	expect(#calls(app, "play")).to_be(2)
	expect(calls(app, "play")[2].key).to_be("line-3")
end)

test("a live meeting's lines don't play", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:click("#row-2-text")
	expect(#calls(app, "play")).to_be(0)
end)

-- Inline rename ------------------------------------------------------------

test("a speaker's chip renames it inline, with known people and Add", function()
	local app = mount()
	local win = stored(app)
	win:click("#row-2-chip")
	app:frames(2)
	expect(win:has("#rename-input")).to_be(true)
	win:fill("#rename-input", "Dav")
	app:frames(2)
	expect(win:text("#rename-option-1")).to_be("David")
	expect(win:text("#rename-option-2")).to_be("Add “Dav”")
	win:key("Enter")
	expect(calls(app, "rename_speakers")[1]).to_equal({ id = ID, names = { S1 = "David" } })
	expect(win:wait("#rename-input", { gone = true })).to_be(true)
end)

test("Down picks the next suggestion; a click takes one; Escape leaves it", function()
	local app = mount()
	local win = stored(app)
	win:click("#row-3-chip")
	app:frames(2)
	win:fill("#rename-input", "Dav")
	app:frames(2)
	win:key("ArrowDown")
	win:key("Enter")
	expect(calls(app, "rename_speakers")[1]).to_equal({ id = ID, names = { S2 = "Dav" } })

	win:click("#row-3-chip")
	app:frames(2)
	win:fill("#rename-input", "pri")
	app:frames(12)
	win:click("#rename-option-1")
	expect(calls(app, "rename_speakers")[2]).to_equal({ id = ID, names = { S2 = "Priya" } })

	win:click("#row-3-chip")
	app:frames(2)
	win:key("Escape")
	expect(win:wait("#rename-input", { gone = true })).to_be(true)
	expect(#calls(app, "rename_speakers")).to_be(2)
end)

test("Me's chip doesn't rename: it is my name from Settings", function()
	local app = mount()
	local win = stored(app)
	expect(win:has("#row-1-chip")).to_be(false)
	expect(win:text("#row-1")).to_be("Tom")
end)

test("Name speakers opens the dialog for the meeting", function()
	local app = mount()
	local win = stored(app)
	win:click("#name-speakers")
	expect(calls(app, "open_name_speakers")[1]).to_be(ID)
end)

-- The dialog --------------------------------------------------------------

local function dialog(app)
	app:set_state("speakers_dialog", fx.speakers_dialog())
	app:set_state("window.speakers", true)
	app:frames(2)
	local win = app:window("speakers")
	-- The dialog fades in (200 ms).
	app:frames(20)
	return win
end

test("the dialog lists each speaker with its chip, two quotes and its name", function()
	local app = mount()
	local win = dialog(app)
	expect(win:text("#speakers-title")).to_be("Name speakers")
	expect(win:text("#speaker-1-chip")).to_be("Me")
	expect(win:text("#speaker-1-quote-1")).to_be("“Today I want to settle the launch date for the mobile app…”")
	expect(win:text("#speaker-1-quote-2")).to_be("“I'll write the release notes and share a draft on Wednesday.”")
	-- Me is Settings' name, locked.
	expect(win:text("#speaker-1-name")).to_be("Tom")
	expect(win:has("#speaker-1-locked")).to_be(true)
	expect(win:has("#speaker-1-field")).to_be(false)
	expect(win:text("#speaker-2-chip")).to_be("Speaker 1")
	expect(app:state("dialog_name.S1")).to_be("Priya")
	-- Unnamed: the AI's hint, one click away.
	expect(win:text("#speaker-3-chip")).to_be("Speaker 2")
	expect(app:state("dialog_name.S2")).to_be("")
	expect(win:text("#speaker-3-suggest")).to_be("David")
	expect(win:has("#speaker-2-suggest")).to_be(false)
end)

test("Save names keeps what changed and closes; the hint fills the field", function()
	local app = mount()
	local win = dialog(app)
	win:scroll("#speakers-list", 1000)
	app:frames(2)
	win:click("#speaker-3-suggest")
	expect(app:state("dialog_name.S2")).to_be("David")
	expect(win:wait("#speaker-3-suggest", { gone = true })).to_be(true)
	win:fill("#speaker-2-field", "Priya Shah")
	win:key("ctrl+Enter")
	expect(calls(app, "rename_speakers")[1]).to_equal({ id = ID, names = { S1 = "Priya Shah", S2 = "David" } })
	expect(app:state("window.speakers")).to_be(false)
end)

test("Skip (Esc) closes without a rename", function()
	local app = mount()
	local win = dialog(app)
	win:fill("#speaker-2-field", "Someone")
	app:frames(2)
	-- The first Escape closes the field's list, the next skips.
	expect(app:state("name_field.speaker-2-field.open")).to_be(true)
	win:key("Escape")
	expect(app:state("name_field.speaker-2-field.open")).to_be(false)
	expect(app:state("window.speakers")).to_be(true)
	win:key("Escape")
	expect(#calls(app, "rename_speakers")).to_be(0)
	expect(app:state("window.speakers")).to_be(false)
end)

test("▶ plays the voice's clip; playing it shows ■ and its progress", function()
	local app = mount()
	local win = dialog(app)
	win:click("#speaker-3-play")
	expect(calls(app, "play")[1]).to_equal({ id = ID, key = "speaker-S2", speaker = "S2" })
	expect(win:has("#speaker-3-progress")).to_be(false)
	app:set_state("playback", { key = "speaker-S2", duration_ms = 8000 })
	expect(win:wait("#speaker-3-progress")).to_be(true)
	expect(win:has("#speaker-3-stop-icon")).to_be(true)
	expect(win:has("#speaker-2-stop-icon")).to_be(false)
	win:click("#speaker-3-play")
	expect(app:state("calls")[2]).to_equal({ name = "stop_playback", arg = nil })
end)

-- Suggestions (pure) ------------------------------------------------------

test("suggestions: words that start with it first, then Add unless it's someone", function()
	local people = { "Anand", "David", "Priya", "Tom Davis" }
	local labels = function(list)
		local out = {}
		for _, s in list do
			table.insert(out, s.label)
		end
		return out
	end
	expect(labels(speakers.suggestions(people, "dav", nil))).to_equal({ "David", "Tom Davis", "Add “dav”" })
	expect(labels(speakers.suggestions(people, "an", nil))).to_equal({ "Anand", "Add “an”" })
	expect(labels(speakers.suggestions(people, " david ", nil))).to_equal({ "David" })
	-- The name it has already isn't offered back.
	expect(labels(speakers.suggestions(people, "", "Priya"))).to_equal({ "Anand", "David", "Tom Davis" })
end)

-- The summary written again --------------------------------------------------

local function on_summary(app)
	local win = stored(app)
	app:set_state("summary", fx.summary())
	win:click("#tab-summary")
	app:frames(30)
	return win
end

test("after a rename only the changed paragraphs wash, and nothing fades in again", function()
	local app = mount()
	local win = on_summary(app)
	-- The first summary fades in; nothing washes.
	expect(win:has("#wash-overview")).to_be(false)

	local s = fx.summary()
	s.decisions[2] = "If load testing still fails, David's team soft-launches in India only."
	s.next_steps[2].owner = "Dave"
	app:set_state("summary", s)
	app:frames(2)
	expect(win:text("#decision-2")).to_be("If load testing still fails, David's team soft-launches in India only.")
	expect(win:has("#wash-decision-2")).to_be(true)
	expect(win:has("#wash-next-2")).to_be(true)
	expect(win:has("#wash-overview")).to_be(false)
	expect(win:has("#wash-decision-1")).to_be(false)
	expect(win:has("#wash-next-1")).to_be(false)
	expect(win:has("#wash-topic-1-bullet-1")).to_be(false)
end)

test("a summary written again while on the transcript doesn't switch tabs", function()
	local app = mount()
	stored(app)
	app:set_state("summary", fx.summary())
	app:set_state("processing", {
		id = ID,
		steps = {
			{ id = "transcribing", label = "Transcribing", status = "done", took_ms = 0, error = "" },
			{ id = "naming_speakers", label = "Naming speakers", status = "done", took_ms = 0, error = "" },
			{ id = "writing_summary", label = "Writing summary", status = "running", took_ms = 0, error = "" },
		},
	})
	app:frames(2)
	expect(app:state("notes_tab")).to_be("transcript")
	local s = fx.summary()
	s.decisions[1] = "Launch on December 1."
	app:set_state("summary", s)
	app:frames(2)
	expect(app:state("notes_tab")).to_be("transcript")
end)
