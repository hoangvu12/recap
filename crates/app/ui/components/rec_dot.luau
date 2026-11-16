--!strict
-- The 6 px recording dot: `rec` and breathing while recording (opacity
-- 1 → .45, 2 s), still and tertiary otherwise (motion_spec §2.4).
local host = require("@app/host")

return function(): Node
	return ui.switch(function()
		return if host.meeting:get().state == "recording" then "on" else "off"
	end, {
		on = ui.div {
			id = "rec-dot",
			w = 6,
			h = 6,
			shrink = 0,
			rounded = 3,
			bg = "$rc.rec",
			opacity = ui.loop { from = 1, to = 0.45, duration = 1000, ease = "inOut(2)" },
		},
		off = ui.div { w = 6, h = 6, shrink = 0, rounded = 3, bg = "$rc.text.tertiary" },
	})
end
