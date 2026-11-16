--!strict
-- The pieces every Settings page is made of (screens/8, 8b): a titled
-- section holding a card of rows, hairlines between them.
local M = {}

-- "Use for", "Speech-to-text"…: a 13/500 title over its card.
function M.Section(props: { title: string, id: string?, [number]: Child }): Node
	return ui.column {
		id = props.id,
		w_full = true,
		gap = 10,
		ui.text { props.title, text_size = 13, weight = 500, color = "$rc.text.primary" },
		ui.column {
			w_full = true,
			px = 12,
			py = 4,
			rounded = 8,
			border = 1,
			border_color = "$rc.stroke.hairline",
			bg = "$rc.bg.card",
			{ table.unpack(props) },
		},
	}
end

-- A row of a card: 48 tall unless said, a hairline above all but the first.
function M.Row(props: { id: string?, first: boolean?, h: number?, [number]: Child }): Node
	return ui.column {
		id = props.id,
		w_full = true,
		shrink = 0,
		ui.div { h = if props.first then 0 else 1, w_full = true, bg = "$rc.stroke.hairline" },
		ui.row {
			w_full = true,
			h = props.h or 48,
			gap = 10,
			items = "center",
			{ table.unpack(props) },
		},
	}
end

-- A quiet outlined label (capability chips: Live, Speakers, Keyterms).
function M.Tag(text: string, id: string?): Node
	return ui.row {
		id = id,
		h = 20,
		px = 6,
		shrink = 0,
		items = "center",
		rounded = 4,
		border = 1,
		border_color = "$rc.stroke.field",
		ui.text { text, literal = true, text_size = 11, color = "$rc.text.secondary" },
	}
end

-- An outlined button (Change, Sign in).
function M.Button(props: { id: string?, label: string, on_click: () -> () }): Node
	return ui.button {
		id = props.id,
		h = 28,
		px = 10,
		shrink = 0,
		rounded = 6,
		bg = "$rc.transparent",
		border = 1,
		border_color = "$rc.stroke.field",
		hover = { bg = "$rc.fill.hover" },
		on_click = props.on_click,
		ui.text { props.label, text_size = 13, weight = 500, color = "$rc.text.primary" },
	}
end

return M
