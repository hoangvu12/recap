--!nonstrict
-- The caption overlay (screens/1 "Live") from a mocked host state.

local function overlay(app, lines)
	app:set_state("meeting", { state = "recording", elapsed_ms = 62000, title = "", started_ms = 0 })
	app:set_state("captions", lines)
	app:set_state("window.overlay", true)
	app:frames(2)
	return app:window("overlay")
end

test("the newest line is at the bottom, final then partial in one paragraph", function()
	local app = mount()
	local win = overlay(app, {
		{ source = "them", final = "Then I'd suggest we launch on December first, with caching in place.", partial = "" },
		{ source = "them", final = "If load testing still fails,", partial = "we fall back to a soft" },
	})
	expect(win:text("#overlay-timer")).to_be("01:02")
	expect(win:text("#caption-0")).to_be("If load testing still fails, we fall back to a soft")
	expect(win:text("#caption-1")).to_be("Then I'd suggest we launch on December first, with caching in place.")
	expect(win:has("#caption-2")).to_be(false)
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
