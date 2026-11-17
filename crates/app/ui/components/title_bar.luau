--!strict
-- A window's title strip: its name on the left, Windows' own minimize,
-- maximize and close over its right (crates/app/src/windows.rs: `Inset`,
-- 138 by 32 points). It moves the window; a double click maximizes it.
-- ADR 0024.
--
-- Settings keeps its own 40-point strip ("‹ › Settings").

-- Windows 11's caption buttons: 3 × 46 by 32 points.
local CONTROLS_W = 138
local H = 32

export type Props = { id: string, title: string }

return function(props: Props): Node
	return ui.row {
		id = props.id,
		h = H,
		w_full = true,
		shrink = 0,
		pl = 16,
		pr = CONTROLS_W,
		items = "center",
		window_drag = true,
		ui.text { props.title, text_size = 12, color = "$rc.text.secondary", truncate = true },
	}
end
