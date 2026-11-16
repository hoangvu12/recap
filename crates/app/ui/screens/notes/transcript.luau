--!strict
-- The live transcript tab: a virtualized list of rows that follows the
-- bottom; scrolled away, a "↓ N new" pill takes you back (motion_spec §2.2).
-- Rows come from the host one at a time (host.row): Luau never holds the
-- whole meeting.
local host = require("@app/host")
local state = require("@app/state")
local actions = require("@app/actions")
local Icon = require("@components/icon")
local TranscriptRow = require("@components/transcript_row")

local function count(): number
	return host.transcript:get().count
end

local function JumpPill(list: Ref): Node
	local unseen = function()
		return math.max(count() - state.seen_count:get(), 0)
	end
	return ui.show(
		function()
			return not state.transcript_at_end:get() and count() > 0
		end,
		ui.row {
			absolute = true,
			left = 0,
			right = 0,
			bottom = 28,
			justify = "center",
			pointer_events = "none",
			ui.button {
				id = "jump-to-live",
				h = 28,
				px = 12,
				gap = 4,
				items = "center",
				rounded = 999,
				bg = "$rc.bg.raised",
				hover = { bg = "$rc.bg.raised" },
				border = 1,
				border_color = "$rc.stroke.field",
				shadow = { { x = 0, y = 8, blur = 10, spread = -6, color = "#0000001A" }, { x = 0, y = 20, blur = 25, spread = -5, color = "#00000033" } },
				on_click = function()
					actions.jump_to_live(list:peek())
				end,
				Icon { name = "arrow-down", size = 14 },
				ui.text {
					function()
						local n = unseen()
						return if n > 0 then `{n} new` else "Live"
					end,
					text_size = 11,
					weight = 500,
					color = "$rc.text.secondary",
				},
			},
		}
	)
end

return function(): Node
	local list = ui.ref()
	-- Resting at the end, every row is seen.
	ui.effect(function()
		if state.transcript_at_end:get() then
			state.seen_count:set(count())
		end
	end)
	return ui.column {
		id = "transcript-tab",
		w_full = true,
		grow = 1,
		relative = true,
		ui.list {
			id = "transcript",
			ref = list,
			w_full = true,
			grow = 1,
			pt = 16,
			count = count,
			measured = true,
			follow = true,
			height = 72,
			at_end = state.transcript_at_end,
			scrollbar = true,
			row = function(i: number)
				return TranscriptRow { index = i }
			end,
		},
		JumpPill(list),
	}
end
