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

test("an overlay toolbar button shows its name when the pointer rests on it", function()
	local app = mount()
	local win = overlay(app)
	win:hover("#tb-stop")
	expect(win:wait('"Stop"')).to_be(true)
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
