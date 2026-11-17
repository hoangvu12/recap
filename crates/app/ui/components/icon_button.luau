--!strict
-- A button that shows only an icon. Its `label` is its name for assistive
-- technology and its tooltip. zgx shows the tooltip when the pointer rests
-- on the button or the keyboard focuses it, and hides it on a press, a key
-- or the pointer leaving.
--
-- `keys` (a shortcut, as keycap labels) follow the name in the tooltip and
-- are its description for assistive technology (keys.spoken).
-- `on` makes it a toggle button: highlighted while on, and `checked` for
-- assistive technology (UI Automation's Toggle). `{id}-on` is there while
-- it is on (tests read it).
--
-- `inline_tip` draws the tooltip inside the window, under the button
-- (`{id}-tip`), instead of zgx's popup. zgx holds every popup while none
-- of Recap's windows is the active one, and the caption overlay never
-- becomes active, so its buttons would show no name on hover (ADR 0023).
local Icon = require("@components/icon")
local keys = require("@lib/keys")

export type Props = {
	id: string,
	-- A name, or a function of one (▶ / ■ that swaps as a clip plays).
	icon: string | () -> string,
	label: string,
	on_click: () -> (),
	on: (() -> boolean)?,
	keys: { string }?,
	-- Points square: 28 by default; the icon is 16 (14 when `outlined`).
	size: number?,
	icon_size: number?,
	rounded: number?,
	bg: string?,
	-- The hover fill; `bg`'s own when it has one.
	hover_bg: string?,
	-- A field's hairline round it (a stepper's − and +).
	outlined: boolean?,
	-- The tooltip's words, when not the label's ("Listen" for "Play Priya").
	tip: string?,
	-- Where the tooltip shows: above by default (zgx's).
	tip_side: ("top" | "bottom" | "left" | "right")?,
	inline_tip: boolean?,
	-- Ids for the icons drawn (tests read them): icon name -> id.
	icon_ids: { [string]: string }?,
	color: string?,
	-- A menu's or popover's trigger: whether what it opens is open.
	expanded: (() -> boolean)?,
}

local function tip_text(props: Props): string
	local words = props.tip or props.label
	if props.keys and #props.keys > 0 then
		return `{words}  {table.concat(props.keys, "+")}`
	end
	return words
end

-- When the last inline tip hid (os.clock): the next one within this long
-- shows at once, as one tooltip handing over to the next (Base UI's delay
-- group).
local last_hidden = -math.huge
local DELAY_MS = 300
local HANDOVER_S = 0.4

-- The inline tooltip under the button, and the hover that shows it.
-- (`owner`, not `id`: a component's `id` lands on its root.)
local InlineTip = ui.component(function(props: { owner: string, text: string, size: number, hovered: State<boolean> }): Node
	local shown = ui.use_state(false)
	local stop: (() -> ())? = nil
	ui.effect(function()
		local inside = props.hovered:get()
		if stop then
			stop()
			stop = nil
		end
		if not inside then
			if shown:peek() then
				last_hidden = os.clock()
				shown:set(false)
			end
			return
		end
		if os.clock() - last_hidden < HANDOVER_S then
			shown:set(true)
		else
			stop = ui.after(DELAY_MS, function()
				shown:set(true)
			end)
		end
	end)
	-- Under the button, its right edge on the button's: the overlay's toolbar
	-- sits at the window's right, where a centred tip would run off.
	return ui.show(
		shown,
		ui.row {
			absolute = true,
			top = props.size + 6,
			right = 0,
			w = 320,
			justify = "end",
			pointer_events = "none",
			aria_hidden = true,
			ui.row {
				id = `{props.owner}-tip`,
				h = 24,
				px = 8,
				gap = 6,
				items = "center",
				rounded = 6,
				bg = "$rc.bg.raised",
				border = 1,
				border_color = "$rc.border.raised",
				shadow = { x = 0, y = 4, blur = 12, color = "$rc.shadow.pop" },
				ui.text { props.text, literal = true, text_size = 12, color = "$rc.text.primary" },
			},
		}
	)
end)

return function(props: Props): Node
	local on = props.on
	local size = props.size or 28
	local tint = function(): string
		if props.color then
			return props.color
		end
		return if on and on() then "$rc.text.primary" else "$rc.text.secondary"
	end
	local icon_size = props.icon_size or (if props.outlined then 14 else 16)
	local function draw(name: string): Node
		return Icon { name = name, size = icon_size, color = tint :: any, id = props.icon_ids and props.icon_ids[name] }
	end
	local icon = props.icon
	local face: Node = if type(icon) == "function"
		then ui.presence(icon, function(name: string): Node
			return draw(name)
		end)
		else draw(icon :: string)
	local hovered: State<boolean>? = if props.inline_tip then ui.state(`inline_tip.{props.id}`, false) else nil
	local button = ui.button {
		id = props.id,
		label = props.label,
		tooltip = if props.inline_tip then nil else tip_text(props),
		tooltip_placement = if props.tip_side and not props.inline_tip then { side = props.tip_side, offset = 6 } else nil,
		description = if props.keys and #props.keys > 0 then keys.spoken(props.keys) else nil,
		checked = on,
		expanded = props.expanded,
		w = size,
		h = size,
		p = 0,
		shrink = 0,
		rounded = props.rounded or 6,
		items = "center",
		justify = "center",
		bg = function()
			if on and on() then
				return "$rc.fill.selected"
			end
			return props.bg or "$rc.transparent"
		end,
		border = if props.outlined then 1 else nil,
		border_color = if props.outlined then "$rc.stroke.field" else nil,
		hover = { bg = props.hover_bg or props.bg or "$rc.fill.hover" },
		on_hover = if hovered
			then function(inside: boolean)
				hovered:set(inside)
			end
			else nil,
		on_press = if hovered
			then function()
				hovered:set(false)
			end
			else nil,
		cursor = "pointer",
		on_click = props.on_click,
		face,
		if on then ui.show(on, ui.div { id = `{props.id}-on`, absolute = true, w = 0, h = 0, aria_hidden = true }) else nil,
	}
	if not hovered then
		return button
	end
	return ui.row {
		relative = true,
		shrink = 0,
		button,
		InlineTip { owner = props.id, text = tip_text(props), size = size, hovered = hovered },
	}
end
