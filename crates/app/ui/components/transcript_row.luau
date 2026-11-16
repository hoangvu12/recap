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

local function PauseMarker(index: number, row: host.Row): Node
	local pause = row.pause :: host.Pause
	return ui.row {
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
	}
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
	if first and first.pause then
		return PauseMarker(index, first)
	end
	local source = if first then first.source else "them"
	local has_words = function()
		local r = row:get()
		return r ~= nil and (r.final ~= "" or r.partial ~= "")
	end

	return ui.column {
		id = `row-{index}`,
		class = "transcript-row",
		w_full = true,
		ui.show(
			has_words,
			ui.column {
				w_full = true,
				px = 16,
				pt = 4,
				pb = 20,
				gap = 10,
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
							return if r then r.final else ""
						end,
						color = if source == "me" then "$rc.text.secondary" else "$rc.text.primary",
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
