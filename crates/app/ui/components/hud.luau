--!strict
-- The HUD pill ("Copied", motion_spec §2.8): centred above the footer for
-- 1.2 s, fade only (150 ms in and out). Id `hud`.
local state = require("@app/state")
local motion = require("@lib/motion")

return function(): Node
	return motion.Presence(function()
		return state.hud:get().text ~= ""
	end, function()
		return ui.row {
			w_full = true,
			justify = "center",
			pointer_events = "none",
			motion.Enter("row", {
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
			}, motion.FADE),
		}
	end, {
		exit = motion.base,
		box = { absolute = true, left = 0, right = 0, bottom = 54, pointer_events = "none" },
	})
end
