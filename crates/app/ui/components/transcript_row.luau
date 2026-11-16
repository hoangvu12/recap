--!strict
-- A live transcript row (DESIGN §9.3): the side's chip and the time, then
-- the text: final words in primary (Me in secondary), the partial after them
-- in tertiary, one paragraph with two colour runs (ADR 0001).
local host = require("@app/host")
local format = require("@lib/format")
local Chip = require("./chip")

export type Props = { index: number }

return ui.component(function(props: Props): Node
	local index = props.index
	-- A final row's text, read once from the host until the row opens again
	-- (its side speaks on): rows that are not open never change.
	local settled: host.Row? = nil
	local row = ui.computed(function(): host.Row?
		local open = host.transcript:get().open[tostring(index)]
		if open then
			settled = nil
			return open
		end
		if settled == nil then
			settled = host.row(index)
		end
		return settled
	end)
	local first = row:peek()
	local source = if first then first.source else "them"

	return ui.column {
		id = `row-{index}`,
		class = "transcript-row",
		w_full = true,
		px = 16,
		pt = 4,
		pb = 20,
		gap = 10,
		ui.row {
			gap = 8,
			items = "center",
			Chip.Source(source),
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
end)
