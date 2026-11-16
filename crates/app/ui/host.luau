--!strict
-- The one place the UI meets the Rust host (crates/app/src/host.rs).
--
-- Rust publishes live state as `ui.shared` keys and answers rows on demand
-- (`ui.call_sync`); the UI asks Rust to do things with `invoke`. Without a
-- host (`zgx test`), `host.connected` stays false: what the UI asks is kept
-- in `calls` and rows come from `mock_rows`, so tests drive every screen from
-- a state they set.

export type Source = "me" | "them"

-- A live transcript row (a turn): its final text, then what is still partial.
export type Row = {
	index: number,
	source: Source,
	start_ms: number,
	final: string,
	partial: string,
	open: boolean,
}

export type MeetingState = "idle" | "recording" | "paused" | "stopped"

export type Meeting = {
	state: MeetingState,
	elapsed_ms: number,
	title: string,
	-- When it started, ms since the epoch (0: not yet).
	started_ms: number,
}

export type Transcript = {
	-- How many rows there are.
	count: number,
	-- The rows still partial, by index (as a string): only these change.
	open: { [string]: Row },
}

export type CaptionLine = { source: Source, final: string, partial: string }

export type Call = { name: string, arg: any }

local M = {}

-- Set true by the Rust host before the scripts load.
M.connected = ui.shared("host.connected", false)

M.meeting = ui.shared("meeting", {
	state = "idle",
	elapsed_ms = 0,
	title = "",
	started_ms = 0,
} :: Meeting)

M.transcript = ui.shared("transcript", { count = 0, open = {} } :: Transcript)

-- The overlay's lines, oldest first: the last few rows it may show.
M.captions = ui.shared("captions", {} :: { CaptionLine })

-- Which windows are open (crates/app/src/windows.rs makes them).
M.windows = {
	notes = ui.shared("window.notes", false),
	overlay = ui.shared("window.overlay", false),
}

-- Without a host: what the UI asked for, and the rows a test serves.
M.calls = ui.state("calls", {} :: { Call })
M.mock_rows = ui.state("mock_rows", {} :: { Row })

-- Ask the host to do `name` (stop, pause, resume…).
function M.invoke(name: string, arg: any?)
	if M.connected:peek() then
		ui.invoke(name, arg)
	else
		M.calls:push({ name = name, arg = arg or ui.null })
	end
end

-- Row `index` (from 1) of the live transcript.
function M.row(index: number): Row?
	local open = M.transcript:peek().open[tostring(index)]
	if open then
		return open
	end
	if M.connected:peek() then
		return ui.call_sync("transcript_row", index)
	end
	return M.mock_rows:peek()[index]
end

return M
