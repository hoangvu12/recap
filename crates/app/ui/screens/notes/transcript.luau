--!strict
-- The live transcript tab: a virtualized list of rows that follows the
-- bottom; scrolled away, a "↓ N new" pill takes you back (motion_spec §2.2).
-- Rows come from the host one at a time (host.row): Luau never holds the
-- whole meeting.
local host = require("@app/host")
local state = require("@app/state")
local actions = require("@app/actions")
local Icon = require("@components/icon")
local Button = require("@components/button")
local TranscriptRow = require("@components/transcript_row")
local Chip = require("@components/chip")
local processing = require("@lib/processing")
local Processing = require("./processing")
local motion = require("@lib/motion")

local function count(): number
	return host.transcript:get().count
end

local function JumpPill(list: Ref): Node
	local unseen = function()
		return math.max(count() - state.seen_count:get(), 0)
	end
	-- Enter: 150 ms fade + 4 px rise; exit: a 100 ms fade.
	return motion.Presence(function(): boolean
		-- Only a live meeting has a live end to go back to.
		local s = host.meeting:get().state
		local live = s == "recording" or s == "paused"
		return live and not state.transcript_at_end:get() and count() > 0
	end, function()
		return ui.row {
			w_full = true,
			justify = "center",
			pointer_events = "none",
			motion.Enter("button", {
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
				-- New rows while away: an accent dot (motion_spec §2.2).
				ui.show(function()
					return unseen() > 0
				end, ui.div { id = "jump-dot", w = 6, h = 6, rounded = 3, bg = "$rc.accent", aria_hidden = true }),
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
			}, { opacity = { 0, 1 }, y = motion.RISE, duration = motion.base, ease = motion.ease.outExpo }),
		}
	end, {
		exit = motion.fast_in,
		box = { absolute = true, left = 0, right = 0, bottom = 28, pointer_events = "none" },
	})
end

-- A refined transcript's speakers (screens/5): their chips, in order.
local function Speakers(): Node
	local chips = ui.computed(function(): { host.SpeakerChip }
		return host.meeting:get().speakers or {}
	end)
	return ui.show(
		function()
			return #chips:get() > 0
		end,
		ui.row {
			id = "speakers",
			w_full = true,
			shrink = 0,
			px = 16,
			pt = 16,
			gap = 4,
			items = "center",
			ui.row {
				grow = 1,
				basis = 0,
				gap = 4,
				wrap = true,
				ui.each(chips, function(chip: host.SpeakerChip, i: number)
					return Chip.Speaker(chip.label, chip.n, `speaker-{i}`)
				end, {
					key = function(chip: host.SpeakerChip): string
						return `{chip.n}|{chip.label}`
					end,
				}),
			},
			-- The Name speakers dialog (screens/6).
			Button {
				id = "name-speakers",
				label = "Name speakers",
				look = "link",
				compact = true,
				weight = 400,
				on_click = actions.name_speakers,
			},
		}
	)
end

return function(): Node
	local list = ui.ref()
	-- Another transcript: no line picked, no rename open.
	local generation = host.transcript:peek().generation or 0
	ui.effect(function()
		local now = host.transcript:get().generation or 0
		if now ~= generation then
			generation = now
			state.selected_row:set(0)
			actions.cancel_rename()
		end
	end)
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
		Processing(),
		Speakers(),
		-- A new transcript (another meeting, the refined one replacing the
		-- live one) is a new list: its rows are read again.
		ui.each(
			ui.computed(function(): { number }
				return { host.transcript:get().generation or 0 }
			end),
			function(_)
				return ui.list {
					id = "transcript",
					ref = list,
					w_full = true,
					grow = 1,
					pt = 16,
					-- The live transcript, until the refined one replaces it (screens/3a).
					opacity = function()
						return if processing.transcribing(host.processing:get()) then 0.45 else 1
					end,
					count = count,
					measured = true,
					follow = true,
					height = 72,
					at_end = state.transcript_at_end,
					scrollbar = true,
					row = function(i: number)
						return TranscriptRow { index = i }
					end,
				}
			end,
			{
				key = function(generation: number): string
					return tostring(generation)
				end,
				w_full = true,
				grow = 1,
				basis = 0,
			}
		),
		JumpPill(list),
	}
end
