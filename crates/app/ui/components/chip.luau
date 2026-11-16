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

return { Chip = Chip, Source = Source }
