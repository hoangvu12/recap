--!nonstrict
-- Accessibility: a way into Settings from each window, tooltips on icon
-- buttons, and controls that say their state (docs/research/
-- research_accessibility.md).
local fx = require("./fixtures")
local pfx = require("./provider_fixtures")

local function home(app)
	app:set_state("mock_details", fx.home_details())
	app:set_state("meetings", fx.home_meetings())
	app:set_state("window.home", true)
	app:frames(2)
	return app:window("home")
end

local function settings_open(app)
	app:frames(2)
	return app:window("settings") ~= nil
end

local function overlay(app)
	app:set_state("meeting", { state = "recording", elapsed_ms = 62000, title = "", started_ms = 0 })
	app:set_state("captions", {})
	app:set_state("window.overlay", true)
	app:frames(2)
	local win = app:window("overlay")
	win:hover("#captions")
	app:frames(20)
	return win
end

-- Settings ------------------------------------------------------------------

test("Home has a Settings button, named with its shortcut when the pointer rests on it", function()
	local app = mount()
	local win = home(app)
	expect(win:has('role:button "Settings"')).to_be(true)
	win:hover("#home-settings")
	expect(win:wait('"Settings  Ctrl+,"')).to_be(true)
	win:click("#home-settings")
	expect(settings_open(app)).to_be(true)
end)

test("the Notes window has a Settings button, and Ctrl+, there opens Settings too", function()
	local app = mount()
	local win = fx.live_meeting(app)
	win:click("#notes-settings")
	expect(settings_open(app)).to_be(true)
	app:set_state("window.settings", false)
	app:frames(2)
	expect(app:window("settings")).to_be_nil()
	win:key("ctrl+,")
	expect(settings_open(app)).to_be(true)
end)

-- Tooltips ------------------------------------------------------------------

test("an overlay toolbar button shows its name, drawn in the overlay, when the pointer rests on it", function()
	local app = mount()
	local win = overlay(app)
	expect(win:has("#tb-stop-tip")).to_be(false)
	win:hover("#tb-stop")
	expect(win:wait("#tb-stop-tip", { text = "Stop" })).to_be(true)
	-- Under the button, inside the overlay.
	local function box(sel)
		local first = string.split(win:tree(sel), "\n")[1]
		local x, y, w, h = string.match(first, "(%-?[%d%.]+),(%-?[%d%.]+) ([%d%.]+)x([%d%.]+)")
		return tonumber(x), tonumber(y), tonumber(w), tonumber(h)
	end
	local bx, by, bw, bh = box("#tb-stop")
	local tx, ty, tw, th = box("#tb-stop-tip")
	expect(ty >= by + bh).to_be(true)
	expect(tx + tw <= 720).to_be(true)
	expect(ty + th <= 120).to_be(true)
	-- Leaving hides it.
	win:hover("#overlay-timer")
	expect(win:wait("#tb-stop-tip", { gone = true })).to_be(true)
end)

test("an overlay tooltip waits for the pointer to rest, and a press hides it", function()
	local app = mount()
	local win = overlay(app)
	win:hover("#tb-notes")
	app:frames(1)
	expect(win:has("#tb-notes-tip")).to_be(false)
	expect(win:wait("#tb-notes-tip", { text = "Show notes" })).to_be(true)
	win:click("#tb-notes")
	app:frames(2)
	expect(win:has("#tb-notes-tip")).to_be(false)
end)

test("the overlay says Reconnecting and errors to screen readers", function()
	local app = mount()
	local win = overlay(app)
	expect(win:text("#overlay-announce")).to_be("")
	app:set_state("live", { status = "reconnecting", provider = "elevenlabs", error = "" })
	app:frames(2)
	expect(win:text("#overlay-announce")).to_be("Reconnecting")
	local line = string.split(win:tree("#overlay-announce"), "\n")[1]
	expect(string.find(line, "status", 1, true)).never.to_be_nil()
	app:set_state("live", { status = "error", provider = "elevenlabs", error = "ElevenLabs stopped" })
	app:frames(2)
	expect(win:text("#overlay-announce-error")).to_be("ElevenLabs stopped")
end)

test("a stepper's minus and plus say what they do", function()
	local app = mount()
	local win = pfx.settings(app)
	win:click("#nav-captions")
	app:frames(3)
	win:hover("#set-caption_size-plus")
	expect(win:wait('"Larger caption size"')).to_be(true)
end)

-- States --------------------------------------------------------------------

test("a switch says whether it is on", function()
	local app = mount()
	local win = pfx.settings(app)
	win:click("#nav-general")
	app:frames(3)
	expect(string.find(win:tree("#set-autostart"), "unchecked", 1, true)).never.to_be_nil()
end)

test("a toggle on the overlay says it is on", function()
	local app = mount()
	local s = table.clone(app:state("settings"))
	s.caption_my_speech = true
	app:set_state("settings", s)
	local win = overlay(app)
	local line = string.split(win:tree("#tb-my-lines"), "\n")[1]
	expect(string.find(line, "unchecked", 1, true)).to_be_nil()
	expect(string.find(line, "checked", 1, true)).never.to_be_nil()
end)

-- Shared components ---------------------------------------------------------

local function first_line(win, sel): string
	return string.split(win:tree(sel), "\n")[1]
end

test("Home has no dead All meetings box", function()
	local app = mount()
	local win = home(app)
	expect(win:has("#home-filter")).to_be(false)
	expect(win:has('"All meetings"')).to_be(false)
end)

test("a select's menu checks the chosen option", function()
	local app = mount()
	local win = pfx.settings(app)
	win:click("#nav-storage")
	app:frames(3)
	win:click("#set-keep_audio_days")
	expect(win:wait("#set-keep_audio_days-0")).to_be(true)
	expect(string.find(first_line(win, "#set-keep_audio_days-0"), " checked", 1, true)).never.to_be_nil()
	expect(string.find(first_line(win, "#set-keep_audio_days-30"), " checked", 1, true)).to_be_nil()
	expect(win:has("#set-keep_audio_days-0-check")).to_be(true)
end)

test("the speakers dialog's People list fills the name field", function()
	local app = mount()
	app:set_state("speakers_dialog", fx.speakers_dialog())
	app:set_state("window.speakers", true)
	app:frames(22)
	local win = app:window("speakers")
	win:scroll("#speakers-list", 1000)
	app:frames(2)
	win:hover("#speaker-3-people")
	expect(win:wait('"People"')).to_be(true)
	win:click("#speaker-3-people")
	expect(win:wait("#speaker-3-person-2")).to_be(true)
	win:click("#speaker-3-person-2")
	app:frames(2)
	expect(app:state("dialog_name.S2")).to_be("Priya")
end)

test("the Settings pages are tabs: the arrows move between them, Enter shows one", function()
	local app = mount()
	local win = pfx.settings(app)
	win:click("#nav-general")
	app:frames(2)
	expect(string.find(first_line(win, "#nav-general"), " focused", 1, true)).never.to_be_nil()
	win:key("ArrowDown")
	app:frames(2)
	expect(string.find(first_line(win, "#nav-general"), " focused", 1, true)).to_be_nil()
	win:key("Enter")
	app:frames(2)
	expect(app:state("settings_page")).never.to_be("general")
end)

test("what the pill says (Copied) is said to screen readers too", function()
	local app = mount()
	local win = home(app)
	expect(win:text("#home-announce")).to_be("")
	win:key("ctrl+k")
	win:key("Enter")
	local said = win:text("#hud")
	expect(said).never.to_be("")
	expect(win:text("#home-announce")).to_be(said)
	expect(string.find(first_line(win, "#home-announce"), "status", 1, true)).never.to_be_nil()
end)
