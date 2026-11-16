--!nonstrict
-- Home search and Recover, the action panel (Ctrl K, screens/9) in Home and
-- the Notes window, Copy as Markdown, Export .md, Delete with its confirm.
local fx = require("./fixtures")

local MARKDOWN = "# Mobile app launch on Dec 1\n\n## Summary\n\nLaunch on December 1.\n\n## Transcript\n\n**Tom** (0:00): Okay.\n"

local function home(app, meetings)
	app:set_state("mock_details", fx.home_details())
	app:set_state("mock_markdown", { launch = { markdown = MARKDOWN, file_name = "Mobile app launch on Dec 1.md" } })
	app:set_state("meetings", meetings or fx.home_meetings())
	app:set_state("window.home", true)
	app:frames(2)
	return app:window("home")
end

local function stored_notes(app)
	app:set_state("mock_rows", fx.named_rows())
	app:set_state("mock_markdown", { launch = { markdown = MARKDOWN, file_name = "Mobile app launch on Dec 1.md" } })
	app:set_state("meeting", { state = "stopped", elapsed_ms = 120000, title = "Mobile app launch on Dec 1", started_ms = fx.STARTED_MS, id = "launch" })
	app:set_state("transcript", { count = 3, open = {} })
	app:set_state("window.notes", true)
	app:frames(2)
	return app:window("notes")
end

local function names(calls)
	local out = {}
	for _, call in calls do
		table.insert(out, call.name)
	end
	return out
end

-- Search ---------------------------------------------------------------------

test("search finds a meeting by the words said in it", function()
	local app = mount()
	-- The host's index: "Dec 1" is in the test meeting's transcript and summary.
	app:set_state("mock_search", { ["Dec 1"] = { "launch" }, ["priya caching"] = { "launch" } })
	local win = home(app)
	win:type("#home-search", "Dec 1")
	expect(win:wait("#meeting-sync", { gone = true })).to_be(true)
	expect(win:has("#meeting-launch")).to_be(true)
	expect(app:state("home_selected")).to_be("launch")

	win:fill("#home-search", "priya caching")
	app:frames(2)
	expect(win:has("#meeting-launch")).to_be(true)
	expect(win:has("#meeting-walk")).to_be(false)
end)

test("a search nothing holds shows no meeting", function()
	local app = mount()
	app:set_state("mock_search", { budget = {} })
	local win = home(app)
	win:type("#home-search", "budget")
	expect(win:wait("#meeting-launch", { gone = true })).to_be(true)
	expect(win:has("#day-1")).to_be(false)
	expect(win:has("#start-notes")).to_be(true)
end)

-- Recover --------------------------------------------------------------------

test("an unfinished meeting with its recording offers Recover; Enter recovers it", function()
	local app = mount()
	local win = home(app)
	expect(win:text("#meeting-crashed-duration")).to_be("Recover")
	win:click("#meeting-crashed")
	win:key("Enter")
	expect(app:state("calls")).to_equal({ { name = "open_meeting", arg = "crashed" } })
end)

test("an unfinished meeting with nothing recorded has no Recover", function()
	local app = mount()
	local list = fx.home_meetings()
	list[4].recover = false
	local win = home(app, list)
	expect(string.match(win:text("#meeting-crashed-title"), "^Unfinished meeting")).never.to_be_nil()
	expect(win:text("#meeting-crashed-duration")).to_be("0:00")
end)

-- The panel ------------------------------------------------------------------

test("Ctrl K opens the panel on the selected meeting with its actions and keys", function()
	local app = mount()
	local win = home(app)
	expect(win:has("#action-panel")).to_be(false)
	win:key("ctrl+k")
	expect(app:state("actions_open")).to_be("home")
	expect(win:text("#action-panel-title")).to_be("Mobile app launch on Dec 1")
	expect(win:text("#action-copy_markdown-label")).to_be("Copy as Markdown")
	expect(win:text("#action-export_markdown-label")).to_be("Export .md…")
	expect(win:text("#action-name_speakers-label")).to_be("Name speakers…")
	expect(win:text("#action-regenerate_summary-label")).to_be("Regenerate summary")
	expect(win:text("#action-show_in_folder-label")).to_be("Show recording in folder")
	expect(win:text("#action-delete_meeting-label")).to_be("Delete meeting")
	expect(win:has("#action-search")).to_be(true)
	-- Esc closes it, nothing done.
	win:key("Escape")
	expect(win:wait("#action-panel", { gone = true })).to_be(true)
	expect(#app:state("calls")).to_be(0)
end)

test("Ctrl K again closes the panel; the footer's Actions opens it too", function()
	local app = mount()
	local win = home(app)
	win:key("ctrl+k")
	win:key("ctrl+k")
	expect(app:state("actions_open")).to_be("")
	win:click("#home-actions")
	expect(app:state("actions_open")).to_be("home")
end)

-- Seen in the real app: a fresh Home (no meeting yet, one live) showed
-- "Actions Ctrl K", which did nothing.
test("an empty Home offers no Actions, and Ctrl K opens nothing", function()
	local app = mount()
	local win = home(app, {})
	expect(win:has("#home-actions")).to_be(false)
	expect(win:has("#open-meeting")).to_be(false)
	win:key("ctrl+k")
	expect(app:state("actions_open")).to_be("")
	-- A meeting arrives (the live one saved): Actions shows and opens.
	app:set_state("meetings", fx.home_meetings())
	app:frames(2)
	expect(win:has("#home-actions")).to_be(true)
	win:click("#home-actions")
	expect(app:state("actions_open")).to_be("home")
end)

test("typing filters the actions; Enter runs the one selected", function()
	local app = mount()
	local win = home(app)
	win:key("ctrl+k")
	win:type("#action-search", "regen")
	app:frames(2)
	expect(win:has("#action-copy_markdown")).to_be(false)
	expect(win:has("#action-regenerate_summary")).to_be(true)
	win:key("Enter")
	expect(app:state("calls")).to_equal({ { name = "regenerate_summary", arg = "launch" } })
	expect(app:state("actions_open")).to_be("")
end)

test("the arrows move through the actions", function()
	local app = mount()
	local win = home(app)
	win:key("ctrl+k")
	win:key("ArrowDown")
	win:key("ArrowDown")
	win:key("ArrowDown")
	win:key("ArrowDown")
	win:key("Enter")
	expect(app:state("calls")).to_equal({ { name = "show_in_folder", arg = "launch" } })
	-- Home's own selection stayed.
	expect(app:state("home_selected")).to_be("launch")
end)

test("Copy as Markdown puts the summary and transcript on the clipboard", function()
	local app = mount()
	local win = home(app)
	win:key("ctrl+k")
	win:key("Enter")
	expect(app:clipboard()).to_be(MARKDOWN)
	expect(win:text("#hud")).to_be("Copied")
	-- The pill goes after 1.2 s.
	expect(win:wait("#hud", { gone = true, timeout_ms = 2000 })).to_be(true)
end)

test("Export writes the Markdown to the file chosen", function()
	local app = mount()
	local win = home(app)
	app:save_to("launch.md")
	win:key("ctrl+e")
	expect(win:wait("#hud", { text = "Exported", timeout_ms = 3000 })).to_be(true)
	expect(app:read_file("launch.md")).to_be(MARKDOWN)
end)

test("each action's keys run it from the window", function()
	local app = mount()
	local win = home(app)
	win:key("ctrl+r")
	win:key("ctrl+shift+r")
	win:key("ctrl+o")
	expect(app:state("calls")).to_equal({
		{ name = "open_name_speakers", arg = "launch" },
		{ name = "regenerate_summary", arg = "launch" },
		{ name = "show_in_folder", arg = "launch" },
	})
	win:key("ctrl+shift+c")
	expect(app:clipboard()).to_be(MARKDOWN)
end)

test("Delete asks first; Enter deletes", function()
	local app = mount()
	local win = home(app)
	win:key("ctrl+k")
	win:type("#action-search", "delete")
	win:key("Enter")
	app:frames(2)
	expect(win:text("#confirm-delete-title")).to_be("Mobile app launch on Dec 1")
	expect(#app:state("calls")).to_be(0)
	win:key("Enter")
	expect(app:state("calls")).to_equal({ { name = "delete_meeting", arg = "launch" } })
	expect(app:state("confirm_delete").id).to_be("")
end)

test("Delete's Cancel keeps the meeting", function()
	local app = mount()
	local win = home(app)
	win:key("ctrl+shift+Backspace")
	app:frames(2)
	expect(win:has("#confirm-delete")).to_be(true)
	win:click("#confirm-delete-no")
	expect(app:state("confirm_delete").id).to_be("")
	expect(#app:state("calls")).to_be(0)
end)

test("with no meeting there is no panel", function()
	local app = mount()
	local win = home(app, {})
	win:key("ctrl+k")
	expect(app:state("actions_open")).to_be("")
	win:key("ctrl+e")
	expect(#app:state("calls")).to_be(0)
end)

-- The Notes window -------------------------------------------------------------

test("a stored meeting's Notes window has the panel and Copy Markdown as its primary", function()
	local app = mount()
	local win = stored_notes(app)
	expect(win:text("#copy-markdown")).to_be("Copy Markdown")
	win:key("ctrl+k")
	expect(app:state("actions_open")).to_be("notes")
	expect(win:text("#action-panel-title")).to_be("Mobile app launch on Dec 1")
	win:key("Escape")
	app:frames(2)
	-- Enter (on the transcript) copies.
	win:key("Enter")
	expect(app:clipboard()).to_be(MARKDOWN)
	win:key("ctrl+r")
	expect(names(app:state("calls"))).to_equal({ "open_name_speakers" })
end)

test("a live meeting has no meeting actions yet", function()
	local app = mount()
	local win = fx.live_meeting(app)
	expect(win:has("#copy-markdown")).to_be(false)
	win:key("ctrl+k")
	expect(app:state("actions_open")).to_be("")
	win:key("ctrl+shift+Backspace")
	expect(app:state("confirm_delete").id).to_be("")
end)
