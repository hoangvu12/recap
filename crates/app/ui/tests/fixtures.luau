--!nonstrict
-- Host state for UI tests: the test meeting (testdata/meeting_tts_script.txt)
-- as the host would publish it a minute in.
local M = {}

M.LINES = {
	{ "me", 0, "Okay, let's get started. Thanks for joining, Priya and David. Today I want to settle the launch date for the mobile app and agree on who owns the remaining work." },
	{ "them", 11000, "Thanks, Tom. Quick status from design. The onboarding screens are done, and the new checkout flow is about ninety percent finished. I still need to fix the dark mode colors on the payment page." },
	{ "them", 24000, "From the backend side, the payments API is ready, but load testing showed slow responses above two thousand users. I think we need one more week to add caching before we go live." },
	{ "me", 37000, "One more week pushes us to December first. Priya, can design finish the dark mode work by then?" },
	{ "them", 44000, "Yes, I can finish dark mode by Friday, so that's not a blocker." },
	{ "them", 49000, "Then I'd suggest we launch on December first, with caching in place. If load testing still fails, we fall back to a soft launch in India only." },
}

-- Rows 1..6 final; row 7 still partial.
function M.rows()
	local rows = {}
	for i, l in M.LINES do
		rows[i] = { index = i, source = l[1], start_ms = l[2], final = l[3], partial = "", open = false }
	end
	return rows
end

M.OPEN = {
	index = 7,
	source = "them",
	start_ms = 62000,
	final = "",
	partial = "Okay, that works for me. Let's make it official, we launch on",
	open = true,
}

-- Nov 16 2026, 10:00 local time is not portable across zones: tests read
-- the date's shape, not its value.
M.STARTED_MS = 1794823200000

-- A Notes window a minute into the test meeting.
function M.live_meeting(app)
	app:set_state("mock_rows", M.rows())
	app:set_state("meeting", { state = "recording", elapsed_ms = 62000, title = "", started_ms = M.STARTED_MS })
	app:set_state("transcript", { count = 7, open = { ["7"] = M.OPEN } })
	app:set_state("window.notes", true)
	app:frames(2)
	return app:window("notes")
end

return M
