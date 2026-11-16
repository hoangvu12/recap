--!strict
-- An icon from the design's icon set (lib/icons.luau), tinted.
local icons = require("@lib/icons")

export type Props = {
	name: string,
	-- Points square; 16 by default.
	size: number?,
	color: string?,
	id: string?,
}

return function(props: Props): Node
	local code = icons.svg[props.name]
	assert(code, `no icon named {props.name}`)
	local size = props.size or 16
	return ui.svg {
		id = props.id,
		code = code,
		w = size,
		h = size,
		shrink = 0,
		tint = props.color or "$rc.text.secondary",
		aria_hidden = true,
	}
end
