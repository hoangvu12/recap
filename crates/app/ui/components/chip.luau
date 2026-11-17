--!strict
-- A speaker chip: hue text on the hue at 10%, full radius, 11/500
-- (DESIGN §9.3 Transcript row). Live rows are Me (purple) or Them (blue);
-- named speakers get their own hues later.
--
-- A speaker renamed (Save names, an inline rename) flashes once: its fill
-- goes from 20% back to 10% over 300 ms (motion_spec §2.9). Every chip of
-- that speaker shown within the flash plays it (`speaker-<n>-flash`).
local host = require("@app/host")
local motion = require("@lib/motion")

export type Props = { label: string, hue: string, id: string?, flash: boolean?, n: number? }

local FLASH_MS = 300

-- The flash: the hue at 10% over the chip's own 10%, fading to nothing.
local Flash = ui.component(function(props: { hue: string, n: number }): Node
	local done = ui.use_state(false)
	ui.after(1, function()
		done:set(true)
	end)
	return ui.div {
		id = `speaker-{props.n}-flash`,
		absolute = true,
		left = 0,
		right = 0,
		top = 0,
		bottom = 0,
		rounded = 999,
		bg = `$rc.{props.hue}.soft`,
		pointer_events = "none",
		aria_hidden = true,
		opacity = motion.tween(done, { from = 1, to = 0, duration = FLASH_MS, ease = motion.ease.out }),
	}
end)

local function Chip(props: Props): Node
	return ui.row {
		id = props.id,
		h = 20,
		px = 8,
		shrink = 0,
		items = "center",
		rounded = 999,
		relative = true,
		bg = `$rc.{props.hue}.soft`,
		if props.flash and not motion.reduced() then Flash { hue = props.hue, n = props.n or 0 } else nil,
		ui.text { props.label, literal = true, text_size = 11, weight = 500, color = `$rc.{props.hue}` },
	}
end

local SOURCES = {
	me = { label = "Me", hue = "purple" },
	them = { label = "Them", hue = "blue" },
}

-- The chip for a live row's side.
local function Source(source: string): Node
	local s = SOURCES[source] or SOURCES.them
	return Chip { label = s.label, hue = s.hue }
end

-- Speakers take hues in this order (screens/7: Tom, Priya, David).
local SPEAKER_HUES = { "purple", "green", "orange", "blue", "yellow" }

-- Each meeting's speakers' labels as last shown, by "<meeting>|<n>", and the
-- label a speaker is flashing with (renamed just now).
local shown: { [string]: string } = {}
local flashing: { [string]: { label: string, until_s: number } } = {}

-- Is speaker `n` labelled `label` a rename just now?
local function renamed(n: number, label: string): boolean
	if n < 1 then
		return false
	end
	local key = `{host.meeting:peek().id or ""}|{n}`
	local flash = flashing[key]
	if flash and flash.label == label and os.clock() < flash.until_s then
		return true
	end
	local before = shown[key]
	shown[key] = label
	if before == nil or before == label then
		return false
	end
	-- A deadline, not a timer: this runs while a row builds, and a timer
	-- started there is stopped when that build runs again (LEARNINGS), so
	-- the flash would never end and every later build would flash again.
	flashing[key] = { label = label, until_s = os.clock() + FLASH_MS / 1000 }
	return true
end

-- The chip for a meeting's speaker `n` (from 1; 0 when not known).
local function Speaker(label: string, n: number, id: string?): Node
	local hue = if n >= 1 then SPEAKER_HUES[(n - 1) % #SPEAKER_HUES + 1] else "blue"
	return Chip { label = label, hue = hue, id = id, flash = renamed(n, label), n = n }
end

return { Chip = Chip, Source = Source, Speaker = Speaker }
