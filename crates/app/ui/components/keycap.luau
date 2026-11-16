--!strict
-- A quiet outlined keycap (Raycast): 18 tall, radius 4, fg 20% border.
-- `id` goes on its text (tests read it).
return function(key: string, id: string?): Node
	return ui.row {
		h = 18,
		min_w = 18,
		px = 4,
		shrink = 0,
		items = "center",
		justify = "center",
		rounded = 4,
		border = 1,
		border_color = "$rc.stroke.field",
		ui.text { key, id = id, literal = true, text_size = 11, color = "$rc.text.secondary" },
	}
end
