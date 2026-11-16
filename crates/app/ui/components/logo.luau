--!strict
-- A provider's logo (lib/logos.luau, from the Provider logos board): brand
-- colours, or `$rc.text.primary` for black brands. With `tile`, on a
-- rounded fill.selected square that many points wide (screens/8b: 28,
-- 8c: 22).
local logos = require("@lib/logos")

export type Props = { name: string, tile: number?, size: number?, id: string? }

return function(props: Props): Node
	local logo = logos.logos[props.name] or logos.logos.compatible
	local size = props.size or 16
	local mark = ui.svg {
		code = logo.svg,
		w = size,
		h = size,
		shrink = 0,
		tint = if logo.tint then "$rc.text.primary" else nil,
		aria_hidden = true,
	}
	if not props.tile then
		return ui.row { id = props.id, w = size, h = size, shrink = 0, mark }
	end
	return ui.row {
		id = props.id,
		w = props.tile,
		h = props.tile,
		shrink = 0,
		items = "center",
		justify = "center",
		rounded = if props.tile >= 28 then 6 else 5,
		bg = "$rc.fill.selected",
		mark,
	}
end
