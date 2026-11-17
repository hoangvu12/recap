--!strict
-- A popover (zgx's `ui.popover`) with Recap's raised panel: anchored to its
-- trigger, written where the trigger is. zgx closes it on Escape or a click
-- outside (which still does what it does there), keeps the keyboard inside
-- while it is open and gives it back as it closes (research_accessibility
-- §1). `open` is a function of the app's state; every close goes to
-- `on_close`.
export type Props = {
	open: () -> boolean,
	on_close: () -> (),
	-- Its accessible name.
	label: string,
	anchor: Node,
	-- Its content, built once.
	build: () -> Node,
	w: number?,
	side: ("top" | "bottom" | "left" | "right")?,
	align: ("start" | "center" | "end")?,
	offset: number?,
	-- false: the panel takes focus rather than its first control.
	autofocus: boolean?,
}

return function(props: Props): Node
	return ui.popover {
		open = props.open,
		on_close = props.on_close,
		label = props.label,
		anchor = props.anchor,
		placement = { side = props.side or "bottom", align = props.align or "start", offset = props.offset or 4 },
		autofocus = props.autofocus,
		w = props.w,
		p = 0,
		gap = 0,
		rounded = 8,
		bg = "$rc.bg.raised",
		border = 1,
		border_color = "$rc.border.raised",
		shadow = { { x = 0, y = 8, blur = 10, spread = -6, color = "#0000001A" }, { x = 0, y = 20, blur = 25, spread = -5, color = "#00000033" } },
		-- Children, not zgx's `build`: with a function `open`, `build`
		-- reads `open:get()` and fails at the first open (rev 24b3587). Built
		-- once, as it first opens; what it shows follows the app's state.
		props.build(),
	}
end
