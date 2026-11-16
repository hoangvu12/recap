--!strict
-- The HUD pill ("Copied", motion_spec §2.8): centred above the footer for
-- 1.2 s, fade only. Id `hud`.
local state = require("@app/state")

return function(): Node
	return ui.show(
		function()
			return state.hud:get().text ~= ""
		end,
		ui.row {
			absolute = true,
			left = 0,
			right = 0,
			bottom = 54,
			justify = "center",
			pointer_events = "none",
			ui.row {
				h = 28,
				px = 12,
				items = "center",
				rounded = 999,
				bg = "$rc.bg.raised",
				border = 1,
				border_color = "$rc.stroke.field",
				ui.text {
					function()
						return state.hud:get().text
					end,
					id = "hud",
					text_size = 12,
					weight = 500,
					color = "$rc.text.primary",
				},
			},
		}
	)
end
