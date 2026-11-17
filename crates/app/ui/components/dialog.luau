--!strict
-- A modal dialog (zgx's `ui.modal`) with Recap's panel look: centred over
-- a dimmed window. zgx closes it on Escape, keeps the keyboard inside and
-- gives it back to what had it as it closes (research_accessibility §4).
--
-- Initial focus: zgx's first control by default. A dialog that confirms
-- something destructive starts on the least destructive button instead
-- (APG): give that button `autofocus = true` (components/button) and the
-- dialog `autofocus = false`.
export type Props = {
	open: () -> boolean,
	on_close: () -> (),
	-- Its accessible name.
	label: string,
	build: () -> Node,
	w: number?,
	bg: string?,
	border_color: string?,
	autofocus: boolean?,
}

return function(props: Props): Node
	return ui.modal {
		open = props.open,
		on_close = props.on_close,
		label = props.label,
		autofocus = props.autofocus,
		w = props.w or 320,
		p = 0,
		gap = 0,
		rounded = 12,
		bg = props.bg or "$rc.bg.raised",
		border = 1,
		border_color = props.border_color or "$rc.border.raised",
		build = props.build,
	}
end
