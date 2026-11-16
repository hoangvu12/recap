--!strict
-- The 6 px recording dot: `rec` and breathing while recording (opacity
-- 1 → .45 → 1, 2 s), still and tertiary otherwise (motion_spec §2.4).
-- Reduced motion: a solid `rec` dot (`rec-dot-solid`).
local host = require("@app/host")
local motion = require("@lib/motion")

local function Dot(id: string, color: string, opacity: any?): Node
	return ui.div { id = id, w = 6, h = 6, shrink = 0, rounded = 3, bg = color, opacity = opacity }
end

return function(): Node
	return ui.switch(function()
		return if host.meeting:get().state == "recording" then "on" else "off"
	end, {
		on = motion.Looping(
			-- There and back: one breath every 2 s.
			Dot("rec-dot", "$rc.rec", ui.loop { from = 1, to = 0.45, duration = 1000, ease = "inOut(2)" }),
			Dot("rec-dot-solid", "$rc.rec")
		),
		off = Dot("rec-dot-still", "$rc.text.tertiary"),
	})
end
