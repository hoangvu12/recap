--!nonstrict
-- The overlay's motion and its reduced-motion fallbacks (motion_spec §2.5):
-- the toolbar's 100 ms rise (instant when reduced), the Reconnecting spinner
-- (still when reduced), the rec dot (solid when reduced), the shift up (it
-- still happens, without the slide).

local SETTINGS = {
	my_name = "",
	theme = "light",
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

local TWO = {
	{ index = 4, source = "them", final = "Then I'd suggest we launch on December first, with caching in place.", partial = "" },
	{ index = 5, source = "them", final = "If load testing still fails,", partial = "we fall back to a soft" },
}

local function overlay(app, opts)
	opts = opts or {}
	if opts.reduced then
		app:set_state("reduce_motion", true)
	end
	app:set_state("settings", SETTINGS)
	app:set_state("meeting", { state = "recording", elapsed_ms = 62000, title = "", started_ms = 0 })
	if opts.live then
		app:set_state("live", opts.live)
	end
	app:set_state("captions", TWO)
	app:set_state("window.overlay", true)
	app:frames(2)
	return app:window("overlay")
end

local function hidden(win, target): boolean
	local first = string.match(win:tree(target), "^[^\n]*") or ""
	return string.find(first, " hidden", 1, true) ~= nil
end

-- Hover until the toolbar mounts (after the 150 ms hover delay).
local function hover_until_toolbar(app, win)
	win:hover("#captions")
	for _ = 1, 30 do
		app:frames(1)
		if win:has("#overlay-toolbar") then
			return
		end
	end
	error("no toolbar")
end

test("the toolbar rises in over 100 ms", function()
	local app = mount()
	local win = overlay(app)
	hover_until_toolbar(app, win)
	expect(hidden(win, "#overlay-toolbar")).to_be(true)
	app:frames(8)
	expect(hidden(win, "#overlay-toolbar")).to_be(false)
end)

test("reduced motion: the toolbar shows at once", function()
	local app = mount()
	local win = overlay(app, { reduced = true })
	hover_until_toolbar(app, win)
	expect(hidden(win, "#overlay-toolbar")).to_be(false)
end)

test("reconnecting: the spinner turns, and stands still under reduced motion", function()
	local live = { status = "reconnecting", provider = "ElevenLabs", error = "" }
	local app = mount()
	local win = overlay(app, { live = live })
	expect(win:has("#overlay-reconnecting-spin")).to_be(true)
	local reduced = mount()
	local still = overlay(reduced, { live = live, reduced = true })
	expect(still:text("#overlay-reconnecting")).to_be("Reconnecting")
	expect(still:has("#overlay-reconnecting-spin")).to_be(false)
end)

test("reduced motion: the overlay's rec dot is solid", function()
	local app = mount()
	local win = overlay(app, { reduced = true })
	expect(win:has("#rec-dot")).to_be(false)
	expect(win:has("#rec-dot-solid")).to_be(true)
end)

test("reduced motion: a new turn still moves the lines up, without the slide", function()
	local app = mount()
	local win = overlay(app, { reduced = true })
	local before = app:state("caption_shift").count
	app:set_state("captions", { TWO[2], { index = 6, source = "them", final = "", partial = "Agreed." } })
	app:frames(1)
	expect(app:state("caption_shift").count).to_be(before + 1)
	expect(win:text("#caption-0")).to_be("Agreed.")
	expect(win:text("#caption-1")).to_be("If load testing still fails, we fall back to a soft")
end)

test("reduced motion: idle still dims the text", function()
	local app = mount()
	local win = overlay(app, { reduced = true })
	app:frames(8 * 60 + 2)
	expect(win:has("#captions-idle")).to_be(true)
end)

test("in the light theme the overlay stays dark", function()
	local app = mount()
	local win = overlay(app)
	expect(app:text("#theme")).to_be("recap-light")
	expect(win:text("#overlay-theme")).to_be("recap-dark")
end)
