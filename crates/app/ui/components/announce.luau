--!strict
-- Says a message to screen readers without showing anything: a live region
-- that stays mounted, its text changing (a region mounted with its text is
-- often not read; research_accessibility §8). Polite by default ("Copied",
-- "Reconnecting"); `urgent` interrupts (an error). Not drawn.
export type Props = {
	id: string,
	text: () -> string,
	urgent: boolean?,
}

return function(props: Props): Node
	return ui.text {
		props.text,
		id = props.id,
		literal = true,
		role = if props.urgent then "alert" else "status",
		live = if props.urgent then "assertive" else "polite",
		absolute = true,
		w = 0,
		h = 0,
		overflow = "clip",
		pointer_events = "none",
	}
end
