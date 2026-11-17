--!strict
-- A 28×16 switch, accent when on (DESIGN §9.3 Switch). It says `checked`
-- (UI Automation's Toggle); its name stays the same as it changes (APG).
-- `{id}-state` is "on" or "off", not drawn (tests read it).
local motion = require("@lib/motion")

export type Props = { id: string, label: string, on: () -> boolean, on_change: (boolean) -> () }

return function(props: Props): Node
	-- The thumb slides (transform only) with Raycast's slight overshoot,
	-- 366 ms; the track's colour eases.
	local on = ui.computed(props.on)
	-- Built again as reduced motion changes (motion.Live): the eases follow.
	return motion.Live(function(): Node
		return ui.button {
			id = props.id,
			label = props.label,
			role = "switch",
			checked = props.on,
			w = 28,
			h = 16,
			p = 0,
			shrink = 0,
			rounded = 999,
			bg = motion.tween(on, { from = "$rc.fill.selected", to = "$rc.accent", duration = motion.fast_in }),
			on_click = function()
				props.on_change(not props.on())
			end,
			ui.row {
				w_full = true,
				h_full = true,
				px = 2,
				items = "center",
				ui.div {
					w = 12,
					h = 12,
					rounded = 999,
					bg = "$rc.switch.thumb",
					x = motion.tween(on, { from = 0, to = 12, duration = motion.switch_ms, ease = motion.ease.switch }),
				},
				ui.text {
					function()
						return if props.on() then "on" else "off"
					end,
					id = `{props.id}-state`,
					w = 0,
					h = 0,
					overflow = "clip",
					opacity = 0,
					aria_hidden = true,
				},
			},
		}
	end)
end
