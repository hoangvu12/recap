--!strict
-- The caption overlay (screens/1 "Live"): always dark, the rec dot and the
-- timer at the top left, then the last lines of what Them said, newest at the
-- bottom. The newest line's final words are primary and its partial
-- tertiary, in one paragraph (ADR 0001); earlier lines are secondary, as are
-- my own lines when shown. Hover toolbar, states and motion: ticket #8.
local host = require("@app/host")
local theme = require("@theme")
local format = require("@lib/format")
local RecDot = require("@components/rec_dot")

local TEXT_SIZE = 20
local LINE_HEIGHT = 28

-- Line `back` from the newest (0 = newest).
local function line(back: number): Node
	local at = function(): host.CaptionLine?
		local lines = host.captions:get()
		return lines[#lines - back]
	end
	local newest = back == 0
	return ui.show(
		function()
			return at() ~= nil
		end,
		ui.rich {
			id = `caption-{back}`,
			class = "caption",
			wrap = true,
			w_full = true,
			text_size = TEXT_SIZE,
			line_height = LINE_HEIGHT,
			weight = 500,
			ui.span {
				function()
					local l = at()
					return if l then l.final else ""
				end,
				color = function()
					local l = at()
					return if newest and l and l.source == "them" then "$rc.text.primary" else "$rc.text.secondary"
				end,
			},
			ui.span {
				function()
					local l = at()
					if l == nil or l.partial == "" then
						return ""
					end
					return if l.final == "" then l.partial else " " .. l.partial
				end,
				-- fg 55% on the dark glass (motion_spec §2.5): between
				-- secondary and tertiary.
				color = "#FFFFFF8C",
			},
		}
	)
end

return function(): Node
	return ui.column {
		id = "overlay-window",
		theme = theme.DARK,
		w_full = true,
		h_full = true,
		rounded = 8,
		border = 1,
		border_color = "$rc.stroke.hairline",
		bg = "$rc.overlay.glass",
		color = "$rc.text.primary",
		font = "$rc.font.ui",
		px = 24,
		pt = 14,
		pb = 16,
		gap = 6,
		window_drag = true,
		ui.row {
			gap = 8,
			items = "center",
			h = 16,
			shrink = 0,
			RecDot(),
			ui.text {
				function()
					return format.clock(host.meeting:get().elapsed_ms)
				end,
				id = "overlay-timer",
				font = "$rc.font.mono",
				text_size = 11,
				color = "$rc.text.secondary",
				tabular = true,
			},
		},
		-- The lines sit on the bottom; older text runs out of the top.
		ui.column {
			id = "captions",
			w_full = true,
			-- The room left under the timer, whatever the text's height.
			grow = 1,
			basis = 0,
			justify = "end",
			overflow = "clip",
			line(2),
			line(1),
			line(0),
		},
	}
end
