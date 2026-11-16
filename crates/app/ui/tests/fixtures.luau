--!nonstrict
-- Host state for UI tests: the test meeting (testdata/meeting_tts_script.txt)
-- as the host would publish it a minute in.
local format = require("../lib/format")

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

-- Local time `days` from today at `hour:min`, in ms (Home groups by local day).
local function at(days: number, hour: number, min: number): number
	local t = os.date("*t") :: any
	return format.local_ms({ year = t.year, month = t.month, day = t.day + days, hour = hour, min = min })
end

M.OVERVIEW = "Tom, Priya and David agreed to launch the mobile app on December 1, one week later than planned, so the backend can add caching. An India-only soft launch is the fallback."

-- Home's list, newest first: two today, two yesterday (one never
-- finished), one three days ago.
function M.home_meetings()
	return {
		{ id = "launch", title = "Mobile app launch on Dec 1", started_ms = at(0, 10, 0), duration_ms = 120000, status = "done" },
		{ id = "sync", title = "Weekly design sync", started_ms = at(0, 9, 0), duration_ms = 1930000, status = "done" },
		{ id = "walk", title = "Checkout flow walkthrough", started_ms = at(-1, 17, 0), duration_ms = 2825000, status = "done" },
		{ id = "crashed", title = "", started_ms = at(-1, 16, 12), duration_ms = 0, status = "recording", recover = true },
		{ id = "review", title = "Load test results review", started_ms = at(-3, 10, 0), duration_ms = 3723000, status = "done" },
	}
end

-- What `meeting_detail(id)` answers.
function M.home_details()
	local out = {}
	for _, m in M.home_meetings() do
		out[m.id] = {
			id = m.id,
			title = m.title,
			overview = "",
			started_ms = m.started_ms,
			duration_ms = m.duration_ms,
			status = m.status,
			speakers = {},
		}
	end
	out.launch.overview = M.OVERVIEW
	out.launch.speakers = { { label = "Tom", n = 1 }, { label = "Priya", n = 2 }, { label = "David", n = 3 } }
	return out
end

-- A stored meeting's rows, named.
function M.named_rows()
	local rows = M.rows()
	local names = { "Tom", "Priya", "David" }
	local out = {}
	for i = 1, 3 do
		local row = table.clone(rows[i])
		row.speaker = names[i]
		row.speaker_n = i
		out[i] = row
	end
	return out
end

-- The test meeting's summary as the host publishes it (screens/4).
function M.summary()
	local function step(i, owner, n, text, due, done)
		return { index = i, owner = owner, owner_n = n, text = text, due = due, done = done or false }
	end
	return {
		id = "20261116-100000",
		overview = M.OVERVIEW,
		topics = {
			{
				heading = "Launch readiness",
				bullets = {
					"Design: onboarding done, checkout ~90%. Dark mode colours on the payment page still open.",
					"Backend: payments API ready, but slow above 2,000 users. Caching needs one more week.",
				},
			},
			{ heading = "Release notes", bullets = { "Nobody owned them last time and they shipped late. Tom owns them now." } },
		},
		next_steps = {
			step(1, "David", 3, "Add caching and rerun the load test", "Before Dec 1"),
			step(2, "David", 3, "Send the load test report to the team", "Thu, Nov 19"),
			step(3, "Priya", 2, "Fix dark mode colours on the payment page", "Fri, Nov 20", true),
			step(4, "Priya", 2, "Book a checkout review with support", "Mon, Nov 23"),
			step(5, "Tom", 1, "Write release notes and share a draft", "Wed, Nov 18"),
			step(6, "", 0, "Tell support about the new flow", ""),
		},
		decisions = { "Launch on December 1, with caching in place.", "If load testing still fails: soft launch in India only." },
	}
end

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
