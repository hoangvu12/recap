--!strict
-- A transcript row (DESIGN §9.3): the side's chip (a stored meeting's: the
-- speaker's) and the time, then
-- the text: final words in primary (Me in secondary), the partial after them
-- in tertiary, one paragraph with two colour runs (ADR 0001).
-- A pause marker row is a dashed hairline with the pause, "15:54 – 15:58"
-- (motion_spec §2.3); a row left without words (an echo taken back) shows
-- nothing.
local host = require("@app/host")
local format = require("@lib/format")
local motion = require("@lib/motion")
local arrivals = require("@lib/arrivals")
local Chip = require("./chip")
local Icon = require("./icon")

export type Props = { index: number }

local function Dashes(): Node
	return ui.div {
		h = 1,
		grow = 1,
		basis = 0,
		border_sides = { 1, 0, 0, 0 },
		border_dash = { 3, 3 },
		border_color = "$rc.stroke.hairline",
	}
end

local function PauseMarker(index: number, row: host.Row, arriving: boolean): Node
	local pause = row.pause :: host.Pause
	-- It appears with a 150 ms fade.
	return ui.row(motion.with({
		id = `row-{index}`,
		class = "transcript-row",
		w_full = true,
		px = 16,
		pt = 0,
		pb = 20,
		gap = 8,
		items = "center",
		Dashes(),
		Icon { name = "pause", size = 12, color = "$rc.text.tertiary" },
		ui.text {
			format.span(pause.paused_ms, pause.resumed_ms),
			id = `row-{index}-pause`,
			font = "$rc.font.mono",
			text_size = 11,
			weight = 500,
			color = "$rc.text.tertiary",
			tabular = true,
		},
		Dashes(),
	}, motion.entrance(arriving, { duration = motion.base, rise = 0, ease = motion.ease.out })))
end

return ui.component(function(props: Props): Node
	local index = props.index
	-- A final row's text, read once from the host until the row opens again
	-- (its side speaks on) or is sent again revised: rows that are not open
	-- never change.
	local settled: host.Row? = nil
	local row = ui.computed(function(): host.Row?
		local t = host.transcript:get()
		local open = t.open[tostring(index)]
		if open then
			settled = nil
			return open
		end
		local revised = t.revised and t.revised[tostring(index)]
		if revised then
			settled = revised
		end
		if settled == nil then
			settled = host.row(index)
		end
		return settled
	end)
	local first = row:peek()
	-- A row that just came in plays its entrance (motion_spec §2.3).
	local arriving = arrivals.arriving(index)
	if first and first.pause then
		return PauseMarker(index, first, arriving)
	end
	local source = if first then first.source else "them"
	local has_words = function()
		local r = row:get()
		return r ~= nil and (r.final ~= "" or r.partial ~= "")
	end

	-- Partial → final (motion_spec §2.1): the words that just turned final
	-- (the final text past `fresh_from`; -1 none) ease from tertiary to the
	-- final colour over 150 ms, then join the final run. Nothing reflows: the
	-- text stays the same, only its colour runs change.
	local fresh_from = ui.use_state(-1)
	local fresh_on = ui.use_state(false)
	local last_final = if first then first.final else ""
	local last_partial = if first then first.partial else ""
	local seq = 0
	ui.effect(function()
		local r = row:get()
		if r == nil then
			return
		end
		local grew = #r.final > #last_final and last_partial ~= ""
		if grew and not motion.reduced() then
			if fresh_from:peek() < 0 then
				fresh_from:set(#last_final)
			end
			fresh_on:set(true)
			seq += 1
			local mine = seq
			ui.after(motion.base, function()
				if mine == seq then
					fresh_from:set(-1)
					fresh_on:set(false)
				end
			end)
		end
		last_final, last_partial = r.final, r.partial
	end)
	local final_color = if source == "me" then "$rc.text.secondary" else "$rc.text.primary"
	-- Its space is taken at once; the content fades in and rises 4 px.
	local entrance = motion.entrance(arriving)

	return ui.column {
		id = `row-{index}`,
		class = "transcript-row",
		w_full = true,
		ui.show(
			has_words,
			ui.column {
				id = `row-{index}-body`,
				w_full = true,
				px = 16,
				pt = 4,
				pb = 20,
				gap = 10,
				opacity = entrance.opacity,
				y = entrance.y,
				ui.row {
					gap = 8,
					items = "center",
					if first and first.speaker
						then Chip.Speaker(first.speaker, first.speaker_n or 0)
						else Chip.Source(source),
					ui.text {
						function()
							local r = row:get()
							return if r then format.clock(r.start_ms) else ""
						end,
						id = `row-{index}-time`,
						font = "$rc.font.mono",
						text_size = 11,
						color = "$rc.text.tertiary",
						tabular = true,
					},
					ui.show(function()
						local r = row:get()
						return r ~= nil and r.open
					end, ui.div { w = 6, h = 6, rounded = 3, bg = "$rc.rec", aria_hidden = true }),
				},
				ui.rich {
					id = `row-{index}-text`,
					wrap = true,
					w_full = true,
					text_size = 13,
					line_height = 20,
					ui.span {
						function()
							local r = row:get()
							if r == nil then
								return ""
							end
							local from = fresh_from:get()
							return if from < 0 then r.final else string.sub(r.final, 1, from)
						end,
						color = final_color,
					},
					ui.span {
						function()
							local r = row:get()
							local from = fresh_from:get()
							return if r and from >= 0 and from < #r.final then string.sub(r.final, from + 1) else ""
						end,
						color = motion.tween(fresh_on, { from = "$rc.text.tertiary", to = final_color }),
					},
					ui.span {
						function()
							local r = row:get()
							if r == nil or r.partial == "" then
								return ""
							end
							return if r.final == "" then r.partial else " " .. r.partial
						end,
						color = "$rc.text.tertiary",
					},
				},
			}
		),
	}
end)
