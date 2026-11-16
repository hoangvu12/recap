--!nonstrict
-- The caption overlay (screens/1) from a mocked host state: live, dim
-- after 8 s, reconnecting, error with "Switch to Deepgram", paused; the
-- hover toolbar; the caption settings applied live.

local SETTINGS = {
	my_name = "",
	theme = "system",
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

local function settings(changes)
	local s = table.clone(SETTINGS)
	for k, v in changes or {} do
		s[k] = v
	end
	return s
end

local function overlay(app, lines, opts)
	opts = opts or {}
	app:set_state("settings", settings(opts.settings))
	app:set_state("meeting", { state = opts.state or "recording", elapsed_ms = 62000, title = "", started_ms = 0 })
	if opts.live then
		app:set_state("live", opts.live)
	end
	app:set_state("captions", lines)
	app:set_state("window.overlay", true)
	app:frames(2)
	return app:window("overlay")
end

local function last_call(app)
	local calls = app:state("calls")
	return calls[#calls]
end

local TWO = {
	{ index = 4, source = "them", final = "Then I'd suggest we launch on December first, with caching in place.", partial = "" },
	{ index = 5, source = "them", final = "If load testing still fails,", partial = "we fall back to a soft" },
}

-- Live --------------------------------------------------------------------

test("the newest line is at the bottom, final then partial in one paragraph", function()
	local app = mount()
	local win = overlay(app, TWO)
	expect(win:text("#overlay-timer")).to_be("01:02")
	expect(win:text("#caption-0")).to_be("If load testing still fails, we fall back to a soft")
	expect(win:text("#caption-1")).to_be("Then I'd suggest we launch on December first, with caching in place.")
	expect(win:has("#caption-2")).to_be(false)
	expect(win:has("#overlay-reconnecting")).to_be(false)
	expect(win:has("#overlay-error")).to_be(false)
end)

test("a line that is all partial shows its words", function()
	local app = mount()
	local win = overlay(app, { { source = "them", final = "", partial = "Okay, that works" } })
	expect(win:text("#caption-0")).to_be("Okay, that works")
end)

test("with my captions on, my lines show among theirs, dimmed", function()
	local app = mount()
	local win = overlay(app, {
		{ source = "them", final = "Then I'd suggest we launch on December first.", partial = "" },
		{ source = "me", final = "Okay, that works for me.", partial = "" },
		{ source = "them", final = "", partial = "Agreed" },
	})
	expect(win:text("#caption-1")).to_be("Okay, that works for me.")
	expect(win:has("#caption-1-me")).to_be(true)
	expect(win:has("#caption-0-me")).to_be(false)
end)

test("captions follow the host as words arrive", function()
	local app = mount()
	local win = overlay(app, { { source = "them", final = "", partial = "Agreed." } })
	app:set_state("captions", { { source = "them", final = "Agreed.", partial = "One more thing," } })
	expect(win:wait("#caption-0", { text = "Agreed. One more thing," })).to_be(true)
end)

-- Settings, live ------------------------------------------------------------

test("it shows as many lines as Settings says, 1 to 3", function()
	local three = {
		{ index = 1, source = "them", final = "one", partial = "" },
		{ index = 2, source = "them", final = "two", partial = "" },
		{ index = 3, source = "them", final = "three", partial = "" },
	}
	local app = mount()
	local win = overlay(app, three, { settings = { caption_lines = 1 } })
	expect(win:text("#caption-0")).to_be("three")
	expect(win:has("#caption-1")).to_be(false)
	app:set_state("settings", settings { caption_lines = 3 })
	app:frames(2)
	expect(win:text("#caption-2")).to_be("one")
end)

test("with partial text off only final words show", function()
	local app = mount()
	local win = overlay(app, TWO, { settings = { partial_text = false } })
	expect(win:text("#caption-0")).to_be("If load testing still fails,")
	app:set_state("captions", { TWO[1], { index = 6, source = "them", final = "", partial = "Who is" } })
	app:frames(2)
	-- A turn still all partial waits; the line before is the newest shown.
	expect(win:text("#caption-0")).to_be(TWO[1].final)
end)

-- Dim after 8 s -------------------------------------------------------------

test("after 8 s without new words the text dims, and the next words undim it", function()
	local app = mount()
	local win = overlay(app, TWO)
	app:frames(7 * 60)
	expect(win:has("#captions-idle")).to_be(false)
	app:frames(62)
	expect(win:has("#captions-idle")).to_be(true)
	app:set_state("captions", { TWO[1], { index = 5, source = "them", final = "If load testing still fails, we fall back", partial = "to" } })
	app:frames(1)
	expect(win:has("#captions-idle")).to_be(false)
end)

-- Motion: shift up on a new turn, not on a partial alone ----------------------

test("a new turn shifts the lines up one line; a partial revised in place doesn't", function()
	local app = mount()
	overlay(app, TWO)
	local before = app:state("caption_shift").count
	app:set_state("captions", { TWO[1], { index = 5, source = "them", final = "If load testing still fails,", partial = "we fall back to a" } })
	app:frames(2)
	expect(app:state("caption_shift").count).to_be(before)
	app:set_state("captions", { TWO[2], { index = 6, source = "them", final = "", partial = "Agreed." } })
	app:frames(2)
	expect(app:state("caption_shift").count).to_be(before + 1)
	expect(app:state("caption_shift").px).to_be(28)
end)

-- Reconnecting --------------------------------------------------------------

test("reconnecting keeps the last line, dimmed, under a Reconnecting line", function()
	local app = mount()
	local win = overlay(app, TWO, { live = { status = "reconnecting", provider = "ElevenLabs", error = "" } })
	expect(win:text("#overlay-reconnecting")).to_be("Reconnecting")
	expect(win:text("#caption-0")).to_be("If load testing still fails, we fall back to a soft")
	expect(win:has("#captions-dimmed")).to_be(true)
	app:set_state("live", { status = "live", provider = "ElevenLabs", error = "" })
	app:frames(2)
	expect(win:has("#overlay-reconnecting")).to_be(false)
	expect(win:has("#captions-dimmed")).to_be(false)
end)

-- Error ---------------------------------------------------------------------

test("a quota error says so and offers to switch to Deepgram", function()
	local app = mount()
	local win = overlay(app, TWO, {
		live = {
			status = "error",
			provider = "ElevenLabs",
			error = "ElevenLabs quota reached",
			switch_to = { id = "deepgram", name = "Deepgram" },
		},
	})
	expect(win:text("#overlay-error")).to_be("ElevenLabs quota reached")
	expect(win:text("#switch-live")).to_be("Switch to Deepgram")
	expect(win:has("#caption-0")).to_be(false)
	-- Recording goes on: the dot still breathes.
	expect(win:has("#rec-dot")).to_be(true)
	win:click("#switch-live")
	expect(last_call(app)).to_equal({ name = "switch_live", arg = { id = "deepgram", name = "Deepgram" } })
end)

test("an error with nothing to switch to only says what failed", function()
	local app = mount()
	local win = overlay(app, TWO, {
		live = { status = "error", provider = "ElevenLabs", error = "Deepgram has no API key" },
	})
	expect(win:text("#overlay-error")).to_be("Deepgram has no API key")
	expect(win:has("#switch-live")).to_be(false)
end)

-- Paused --------------------------------------------------------------------

test("paused shows Paused and the resume hotkey under the last line", function()
	local app = mount()
	local win = overlay(app, TWO, { state = "paused", settings = { hotkeys = { start_notes = "", pause = "Ctrl+Shift+P", click_through = "", captions = "" } } })
	expect(win:text("#overlay-paused")).to_be("Paused")
	expect(win:text("#overlay-paused-key-1")).to_be("Ctrl")
	expect(win:text("#overlay-paused-key-2")).to_be("Shift")
	expect(win:text("#overlay-paused-key-3")).to_be("P")
	-- Two lines: the Paused line takes the newest's place.
	expect(win:text("#caption-0")).to_be("If load testing still fails, we fall back to a soft")
	expect(win:has("#caption-1")).to_be(false)
	expect(win:has("#rec-dot-still")).to_be(true)
end)

-- Hover toolbar -------------------------------------------------------------

test("hovering shows the toolbar after a moment", function()
	local app = mount()
	local win = overlay(app, TWO)
	expect(win:has("#overlay-toolbar")).to_be(false)
	win:hover("#captions")
	app:frames(2)
	expect(win:has("#overlay-toolbar")).to_be(false)
	app:frames(12)
	expect(win:has("#overlay-toolbar")).to_be(true)
end)

local function toolbar(app, opts)
	local win = overlay(app, TWO, opts)
	win:hover("#captions")
	app:frames(20)
	return win
end

test("the toolbar pauses, stops and opens the notes", function()
	local app = mount()
	local win = toolbar(app)
	win:click("#tb-pause")
	expect(last_call(app).name).to_be("pause")
	win:click("#tb-stop")
	expect(last_call(app).name).to_be("stop")
	win:click("#tb-notes")
	expect(app:state("window.notes")).to_be(true)
end)

test("while paused the toolbar's first button resumes", function()
	local app = mount()
	local win = toolbar(app, { state = "paused" })
	win:click("#tb-pause")
	expect(last_call(app).name).to_be("resume")
end)

test("the toolbar's text size and my lines change the settings", function()
	local app = mount()
	local win = toolbar(app, { settings = { caption_size = 20 } })
	win:click("#tb-larger")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { caption_size = 22 } })
	win:click("#tb-smaller")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { caption_size = 18 } })
	win:click("#tb-my-lines")
	expect(last_call(app)).to_equal({ name = "set_settings", arg = { caption_my_speech = true } })
end)

test("the toolbar pins and turns click-through on, and shows them on", function()
	local app = mount()
	local win = toolbar(app)
	expect(win:has("#tb-pin-on")).to_be(true)
	expect(win:has("#tb-click-through-on")).to_be(false)
	win:click("#tb-pin")
	expect(last_call(app)).to_equal({ name = "set_overlay_pinned", arg = false })
	win:click("#tb-click-through")
	expect(last_call(app).name).to_be("toggle_click_through")
	-- The hotkey or the button turned it on: shown in the toolbar and by
	-- the timer.
	app:set_state("overlay", { click_through = true, pinned = true })
	app:frames(2)
	expect(win:has("#tb-click-through-on")).to_be(true)
	expect(win:has("#overlay-click-through")).to_be(true)
end)
