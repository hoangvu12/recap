--!strict
-- The one place the UI meets the Rust host (crates/app/src/host.rs).
--
-- Rust publishes live state as `ui.shared` keys and answers rows on demand
-- (`ui.call_sync`); the UI asks Rust to do things with `invoke`. Without a
-- host (`zgx test`), `host.connected` stays false: what the UI asks is kept
-- in `calls` and rows come from `mock_rows`, so tests drive every screen from
-- a state they set.

export type Source = "me" | "them"

-- A pause in a meeting, wall clock (ms since the epoch).
export type Pause = { paused_ms: number, resumed_ms: number }

-- A live transcript row (a turn): its final text, then what is still partial.
-- A pause marker row has `pause` and no text; a row whose words were taken
-- back (an echo) has no text either.
export type Row = {
	pause: Pause?,
	index: number,
	source: Source,
	start_ms: number,
	final: string,
	partial: string,
	open: boolean,
	-- A stored meeting's speaker (name or "Speaker N") and its place; live
	-- rows have none (their chip is Me or Them).
	speaker: string?,
	speaker_n: number?,
}

export type MeetingState = "idle" | "recording" | "paused" | "stopped"

export type Meeting = {
	state: MeetingState,
	elapsed_ms: number,
	title: string,
	-- When it started, ms since the epoch (0: not yet).
	started_ms: number,
	-- The stored meeting's id ("" when it isn't saved).
	id: string?,
	-- What the user should know now: a provider failed, the backup took
	-- over, the meeting can't be saved ("" or nil: nothing).
	notice: string?,
	-- A refined transcript's speakers, in order (the header's chips); empty
	-- for a live one.
	speakers: { SpeakerChip }?,
}

-- After Stop: a step of the meeting's processing (crates/app/src/processing.rs).
export type StepStatus = "pending" | "running" | "done" | "error"
export type Step = {
	id: "transcribing" | "naming_speakers" | "writing_summary",
	label: string,
	status: StepStatus,
	-- How long it took, once done.
	took_ms: number,
	-- Why it failed ("" unless status is "error").
	error: string,
}

-- The processing of the meeting the Notes window shows; no steps when
-- there is none to show (live, or done).
export type Processing = { id: string, steps: { Step } }

-- The meeting's summary (crates/app/src/summary.rs: view); id "" when it
-- has none.
export type Topic = { heading: string, bullets: { string } }
export type NextStep = {
	-- From 1: what tick_next_step names.
	index: number,
	-- Who does it ("" nobody), and their speaker's place (from 1; 0: not a
	-- speaker), which picks the chip's hue.
	owner: string,
	owner_n: number,
	text: string,
	-- "Thu, Nov 19"; "" when none.
	due: string,
	done: boolean,
}
export type Summary = {
	id: string,
	overview: string,
	topics: { Topic },
	next_steps: { NextStep },
	decisions: { string },
}

export type MeetingStatus = "recording" | "processing" | "done"

-- A stored meeting in Home's list.
export type MeetingItem = {
	id: string,
	title: string,
	started_ms: number,
	duration_ms: number,
	-- Not "done" (and not live): it never finished, Recover it.
	status: MeetingStatus,
}

-- A speaker chip: the label, and its place (from 1), which picks the hue.
export type SpeakerChip = { label: string, n: number }

-- Home's detail pane for one meeting.
export type MeetingDetail = {
	id: string,
	title: string,
	overview: string,
	started_ms: number,
	duration_ms: number,
	status: MeetingStatus,
	speakers: { SpeakerChip },
}

export type Transcript = {
	-- How many rows there are.
	count: number,
	-- The rows still partial, by index (as a string): only these change.
	open: { [string]: Row },
	-- Final rows sent again because they changed (an echo taken back).
	revised: { [string]: Row }?,
	-- Changes when the transcript is another one (another meeting, or the
	-- refined one replacing the live one): read every row again.
	generation: number?,
}

-- A caption line: its transcript row's index (a new one is a new turn),
-- its final words and what is still partial.
export type CaptionLine = { index: number?, source: Source, final: string, partial: string }

-- How Them's live captions are doing (the overlay's state): captioning,
-- reconnecting (the last line kept), or failed (`error` says how, and
-- `switch_to` is the engine to offer instead).
export type LiveChoice = { id: string, name: string }
export type LiveCaptions = {
	status: "live" | "reconnecting" | "error",
	provider: string,
	error: string,
	switch_to: LiveChoice?,
}

-- The overlay window's own switches (crates/app/src/overlay.rs).
export type OverlayWindow = { click_through: boolean, pinned: boolean }

-- Providers (crates/providers: Providers::view). Ids are the registry's.
export type JobId = "live" | "transcript" | "summary"
export type Family = "stt" | "ai"

-- A job and its main and backup (provider ids).
export type JobChoice = {
	id: JobId,
	label: string,
	family: Family,
	-- The capabilities a speech engine needs for it ("live", "batch",
	-- "diarize", "keyterms", "word_timestamps").
	needs: { string },
	main: string?,
	backup: string?,
}

-- What a form asks for.
export type Fields = { key: boolean, model: boolean, base_url: boolean, command: boolean, name: boolean }

export type Capabilities = {
	live: boolean,
	batch: boolean,
	diarize: boolean,
	keyterms: boolean,
	word_timestamps: boolean,
}

export type TestOutcome = { ok: boolean, latency_ms: number, error: string? }

-- A provider: a speech engine (always there) or an AI provider the user
-- added.
export type ProviderRow = {
	id: string,
	preset: string,
	kind: string,
	family: Family,
	name: string,
	-- A logo of lib/logos.luau.
	logo: string,
	capabilities: Capabilities,
	fields: Fields,
	key_hint: string,
	-- The key, masked ("sk_••••3f9a"); "" without one. Never the key.
	key_mask: string,
	-- Beside the name: a model, or a command's program.
	subtitle: string,
	-- At the end: the masked key, or where a local one runs.
	trailing: string,
	model: string,
	base_url: string,
	command: string,
	input: "stdin" | "file",
	-- Its last Test this run.
	tested: TestOutcome?,
}

export type Group = "speech" | "api" | "local" | "cli" | "custom"

-- What a provider can be added from (screens/8c).
export type Preset = {
	id: string,
	name: string,
	logo: string,
	group: Group,
	kind: string,
	fields: Fields,
	base_url: string,
	model: string,
	command: string,
	detail: string,
	key_hint: string,
}

export type Providers = {
	jobs: { JobChoice },
	stt: { ProviderRow },
	ai: { ProviderRow },
	presets: { Preset },
}

-- A provider as its form has it (crates/providers: Draft): `id` when
-- saved, `key` only when typed.
export type Draft = {
	id: string?,
	preset: string,
	name: string?,
	model: string?,
	base_url: string?,
	command: string?,
	input: ("stdin" | "file")?,
	key: string?,
}

-- The Test button of the form `target` (a provider id, or "new:<preset>").
export type ProviderTest = {
	target: string,
	status: "idle" | "running" | "ok" | "error",
	latency_ms: number,
	error: string,
}

-- The user's settings (crates/storage: Settings; crates/app/src/user_settings.rs).
export type Hotkeys = { start_notes: string, pause: string, click_through: string, captions: string }
export type Settings = {
	my_name: string,
	theme: "system" | "dark" | "light",
	autostart: boolean,
	caption_my_speech: boolean,
	caption_size: number,
	caption_lines: number,
	partial_text: boolean,
	hide_from_share: boolean,
	-- A capture device's id; "" is the default communications device.
	microphone: string,
	-- "system", or an app's executable ("ms-teams.exe").
	meeting_audio: string,
	suggest_on_call: boolean,
	glossary: { string },
	-- "Ctrl+Alt+N"; "" is none.
	hotkeys: Hotkeys,
	-- "" is Recap's home folder.
	storage_folder: string,
	-- 0 keeps audio.
	keep_audio_days: number,
}

-- Why the last change to `field` ("glossary", "hotkey.<id>", "storage",
-- "save") was refused; "" when it wasn't.
export type SettingsError = { field: string, message: string }

export type Microphone = { id: string, name: string }

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

-- The Notes window's meeting's processing (Transcribing, Naming speakers,
-- Writing summary).
M.processing = ui.shared("processing", { id = "", steps = {} } :: Processing)

-- The Notes window's meeting's summary.
M.summary = ui.shared("summary", {
	id = "",
	overview = "",
	topics = {},
	next_steps = {},
	decisions = {},
} :: Summary)

-- The overlay's lines, oldest first: the last few rows it may show.
M.captions = ui.shared("captions", {} :: { CaptionLine })

-- How the live captions are doing.
M.live = ui.shared("live", { status = "live", provider = "", error = "" } :: LiveCaptions)

-- The overlay's click-through and pin.
M.overlay = ui.shared("overlay", { click_through = false, pinned = true } :: OverlayWindow)

-- The stored meetings, newest first (Home).
M.meetings = ui.shared("meetings", {} :: { MeetingItem })

-- The providers and each job's main and backup (Settings, Providers).
M.providers = ui.shared("providers", { jobs = {}, stt = {}, ai = {}, presets = {} } :: Providers)

-- The open provider form's Test.
M.provider_test = ui.shared("provider_test", {
	target = "",
	status = "idle",
	latency_ms = 0,
	error = "",
} :: ProviderTest)

-- The user's settings, as saved (Settings' pages but Providers).
M.settings = ui.shared("settings", {
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
} :: Settings)

M.settings_error = ui.shared("settings_error", { field = "", message = "" } :: SettingsError)

-- The active microphones (Audio).
M.microphones = ui.shared("microphones", {} :: { Microphone })

-- Hotkeys the system refused (another app holds them), by id: what to say.
M.hotkey_conflicts = ui.shared("hotkey_conflicts", {} :: { [string]: string })

-- A call toast to show (crates/app/src/calls.rs): a new `seq` is a new one.
-- Clicking it asks the host for `call_toast_clicked`.
export type CallToast = { seq: number, title: string, body: string }
M.call_toast = ui.shared("call_toast", { seq = 0, title = "", body = "" } :: CallToast)

-- Which windows are open (crates/app/src/windows.rs makes them).
M.windows = {
	home = ui.shared("window.home", false),
	notes = ui.shared("window.notes", false),
	overlay = ui.shared("window.overlay", false),
	settings = ui.shared("window.settings", false),
}

-- Without a host: what the UI asked for, and the rows and details a test
-- serves.
M.calls = ui.state("calls", {} :: { Call })
M.mock_rows = ui.state("mock_rows", {} :: { Row })
M.mock_details = ui.state("mock_details", {} :: { [string]: MeetingDetail })

-- Meeting `id`'s detail pane.
function M.detail(id: string): MeetingDetail?
	if M.connected:peek() then
		return ui.call_sync("meeting_detail", id)
	end
	return M.mock_details:peek()[id]
end

-- Ask the host to do `name` (stop, pause, resume, start_notes,
-- open_meeting…).
function M.invoke(name: string, arg: any?)
	if M.connected:peek() then
		ui.invoke(name, arg)
	else
		M.calls:push({ name = name, arg = if arg == nil then ui.null else arg })
	end
end

-- Ask the host for `name`, an async action whose answer comes back as
-- shared state (test_provider). Without a host it is kept in `calls`.
function M.call(name: string, arg: any?)
	if M.connected:peek() then
		ui.call(name, arg)
	else
		M.calls:push({ name = name, arg = arg or ui.null })
	end
end

-- Row `index` (from 1) of the live transcript.
function M.row(index: number): Row?
	local t = M.transcript:peek()
	local open = t.open[tostring(index)] or (t.revised and t.revised[tostring(index)])
	if open then
		return open
	end
	if M.connected:peek() then
		return ui.call_sync("transcript_row", index)
	end
	return M.mock_rows:peek()[index]
end

return M
