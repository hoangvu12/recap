--!strict
-- A speaker chip: hue text on the hue at 10%, full radius, 11/500
-- (DESIGN §9.3 Transcript row). Live rows are Me (purple) or Them (blue);
-- named speakers get their own hues later.
export type Props = { label: string, hue: string, id: string? }

local function Chip(props: Props): Node
	return ui.row {
		id = props.id,
		h = 20,
		px = 8,
		shrink = 0,
		items = "center",
		rounded = 999,
		bg = `$rc.{props.hue}.soft`,
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

-- The chip for a meeting's speaker `n` (from 1; 0 when not known).
local function Speaker(label: string, n: number, id: string?): Node
	local hue = if n >= 1 then SPEAKER_HUES[(n - 1) % #SPEAKER_HUES + 1] else "blue"
	return Chip { label = label, hue = hue, id = id }
end

return { Chip = Chip, Source = Source, Speaker = Speaker }
