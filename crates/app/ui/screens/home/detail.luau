--!strict
-- Home's detail pane: the selected meeting's title and overview, then its
-- Information (date, duration, speakers), each row over a hairline.
local host = require("@app/host")
local format = require("@lib/format")
local meetings = require("@lib/meetings")
local Chip = require("@components/chip")

export type Props = { selected: ReadOnlyState<string> }

local function Info(label: string, value: Child): Node
	return ui.column {
		w_full = true,
		ui.row {
			h = 32,
			w_full = true,
			items = "center",
			ui.text { label, grow = 1, basis = 0, text_size = 13, color = "$rc.text.secondary" },
			value,
		},
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
	}
end

local function value_text(id: string, text: string): Node
	return ui.text { text, id = id, literal = true, text_size = 13, weight = 500, color = "$rc.text.primary", tabular = true }
end

local function Pane(d: host.MeetingDetail): Node
	local speakers = {}
	for i, s in d.speakers do
		speakers[i] = Chip.Speaker(s.label, s.n, `speaker-{i}`)
	end
	return ui.column {
		id = "detail",
		w_full = true,
		gap = 16,
		ui.column {
			w_full = true,
			gap = 8,
			ui.text {
				meetings.title(d),
				id = "detail-title",
				literal = true,
				text_size = 16,
				weight = 600,
				color = "$rc.text.primary",
				truncate = true,
			},
			if d.overview ~= ""
				then ui.text {
					d.overview,
					id = "detail-overview",
					literal = true,
					wrap = true,
					w_full = true,
					text_size = 13,
					line_height = 20,
					color = "$rc.text.secondary",
				}
				else nil,
		},
		ui.column {
			w_full = true,
			ui.row {
				h = 24,
				items = "center",
				ui.text { "Information", text_size = 11, weight = 500, color = "$rc.text.secondary" },
			},
			Info("Date", value_text("detail-date", format.date(d.started_ms))),
			Info("Duration", value_text("detail-duration", format.duration(d.duration_ms))),
			if #speakers > 0 then Info("Speakers", ui.row { gap = 4, speakers }) else nil,
		},
	}
end

return function(props: Props): Node
	-- The pane is rebuilt for each meeting selected; the list changing
	-- (a title, a summary) reads it again.
	local detail = ui.computed(function(): { host.MeetingDetail }
		local id = props.selected:get()
		host.meetings:get()
		local d = if id ~= "" then host.detail(id) else nil
		return if d then { d } else {}
	end)
	return ui.scroll {
		id = "home-detail",
		grow = 1,
		basis = 0,
		h_full = true,
		ui.column {
			w_full = true,
			p = 16,
			ui.each(detail, function(d: host.MeetingDetail)
				return Pane(d)
			end, {
				key = function(d: host.MeetingDetail): string
					return `{d.id}|{d.title}|{d.overview}|{#d.speakers}`
				end,
				w_full = true,
			}),
		},
	}
end
