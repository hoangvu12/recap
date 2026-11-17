--!strict
-- A button that shows only an icon. Its `label` is its name for assistive
-- technology and its tooltip. zgx shows the tooltip when the pointer rests
-- on the button or the keyboard focuses it, and hides it on a press, a key
-- or the pointer leaving.
--
-- `keys` (a shortcut, as keycap labels) follow the name in the tooltip.
-- `on` makes it a toggle button: highlighted while on, and `checked` for
-- assistive technology (UI Automation's Toggle). `{id}-on` is there while
-- it is on (tests read it).
local Icon = require("@components/icon")

export type Props = {
	id: string,
	icon: string,
	label: string,
	on_click: () -> (),
	on: (() -> boolean)?,
	keys: { string }?,
	-- Points square: 28 by default; the icon is 16 (14 when `outlined`).
	size: number?,
	-- A field's hairline round it (a stepper's − and +).
	outlined: boolean?,
	-- Where the tooltip shows: above by default (zgx's).
	tip_side: ("top" | "bottom" | "left" | "right")?,
	color: string?,
}

local function tip(props: Props): string
	if props.keys and #props.keys > 0 then
		return `{props.label}  {table.concat(props.keys, "+")}`
	end
	return props.label
end

return function(props: Props): Node
	local on = props.on
	local tint = function(): string
		if props.color then
			return props.color
		end
		return if on and on() then "$rc.text.primary" else "$rc.text.secondary"
	end
	return ui.button {
		id = props.id,
		label = props.label,
		tooltip = tip(props),
		tooltip_placement = if props.tip_side then { side = props.tip_side, offset = 6 } else nil,
		checked = on,
		w = props.size or 28,
		h = props.size or 28,
		p = 0,
		shrink = 0,
		rounded = 6,
		items = "center",
		justify = "center",
		bg = function()
			return if on and on() then "$rc.fill.selected" else "$rc.transparent"
		end,
		border = if props.outlined then 1 else nil,
		border_color = if props.outlined then "$rc.stroke.field" else nil,
		hover = { bg = "$rc.fill.hover" },
		on_click = props.on_click,
		Icon { name = props.icon, size = if props.outlined then 14 else 16, color = tint :: any },
		if on then ui.show(on, ui.div { id = `{props.id}-on`, absolute = true, w = 0, h = 0, aria_hidden = true }) else nil,
	}
end
