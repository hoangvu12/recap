--!strict
-- The Summary tab (screens/4, Wispr's layout): the overview, a section per
-- topic, the next steps as checkboxes with the owner's chip and the due
-- date, the decisions. Sections fade in (240 ms, 40 ms apart, all within
-- 500 ms; DESIGN §9.4) when a summary arrives. While the meeting processes
-- the card sits on top; a meeting without a summary offers to write one.
local host = require("@app/host")
local actions = require("@app/actions")
local processing = require("@lib/processing")
local Chip = require("@components/chip")
local Icon = require("@components/icon")
local Processing = require("./processing")

-- The fade: each section 240 ms, starting 40 ms after the one above; the
-- last starts by 260 ms, so all are in within 500 ms.
local FADE_MS = 240
local STAGGER_MS = 40
local LAST_START_MS = 260

local function fade_in(order: number): (Element) -> ()
	return function(el: Element)
		anime.animate(el, {
			opacity = { 0, 1 },
			y = { 4, 0 },
			duration = FADE_MS,
			delay = math.min((order - 1) * STAGGER_MS, LAST_START_MS),
			ease = "out(2)",
		})
	end
end

local function Header(text: string, id: string?): Node
	return ui.row {
		h = 32,
		w_full = true,
		items = "end",
		pb = 6,
		ui.text { text, id = id, literal = true, text_size = 11, weight = 500, color = "$rc.text.secondary" },
	}
end

local function Body(text: string, id: string?): Node
	return ui.text {
		text,
		id = id,
		literal = true,
		w_full = true,
		wrap = true,
		text_size = 13,
		line_height = 20,
		color = "$rc.text.primary",
	}
end

local function Bullet(text: string, id: string): Node
	return ui.row {
		w_full = true,
		gap = 10,
		py = 3,
		items = "start",
		ui.div { w = 4, h = 4, mt = 8, ml = 2, rounded = 2, shrink = 0, bg = "$rc.text.tertiary", aria_hidden = true },
		ui.text {
			text,
			id = id,
			literal = true,
			grow = 1,
			basis = 0,
			wrap = true,
			text_size = 13,
			line_height = 20,
			color = "$rc.text.primary",
		},
	}
end

local function Section(order: number, id: string, children: { Node }): Node
	return ui.column {
		id = id,
		w_full = true,
		pb = 12,
		on_mount = fade_in(order),
		children,
	}
end

-- A next step: the box, the task, the owner's chip and when (DESIGN §9.3
-- Checkbox item). A click anywhere on it ticks it.
local function NextStep(step: host.NextStep): Node
	local i = step.index
	local done = function(): boolean
		local s = host.summary:get().next_steps[i]
		return s ~= nil and s.done
	end
	local meta: { Node } = {}
	if step.owner ~= "" then
		table.insert(meta, Chip.Speaker(step.owner, step.owner_n, `next-{i}-owner`))
	end
	if step.due ~= "" then
		table.insert(meta, ui.text {
			step.due,
			id = `next-{i}-due`,
			literal = true,
			text_size = 12,
			color = "$rc.text.tertiary",
		})
	end
	return ui.button {
		id = `next-{i}`,
		role = "checkbox",
		selected = done,
		w_full = true,
		py = 6,
		px = 0,
		gap = 10,
		items = "start",
		justify = "start",
		rounded = 0,
		bg = "$rc.transparent",
		hover = { bg = "$rc.transparent" },
		on_click = function()
			actions.tick_next_step(i, not done())
		end,
		ui.div {
			id = `next-{i}-box`,
			w = 16,
			h = 16,
			mt = 2,
			shrink = 0,
			rounded = 4,
			items = "center",
			justify = "center",
			border = function()
				return if done() then 0 else 1
			end,
			border_color = "$rc.stroke.field",
			bg = function()
				return if done() then "$rc.accent" else "$rc.transparent"
			end,
			ui.show(done, Icon { name = "check", size = 12, color = "$rc.on.accent", id = `next-{i}-checked` }),
		},
		ui.column {
			grow = 1,
			basis = 0,
			gap = 6,
			ui.text {
				step.text,
				id = `next-{i}-text`,
				literal = true,
				w_full = true,
				wrap = true,
				text_size = 13,
				line_height = 20,
				color = function()
					return if done() then "$rc.text.tertiary" else "$rc.text.primary"
				end,
			},
			if #meta > 0 then ui.row { gap = 8, items = "center", meta } else nil,
		},
	}
end

-- One summary, its sections in order (each fades in).
local function Content(s: host.Summary): Node
	local sections: { Node } = {}
	local function add(id: string, children: { Node })
		table.insert(sections, Section(#sections + 1, id, children))
	end
	if s.overview ~= "" then
		add("summary-overview", { Header("Overview"), Body(s.overview, "summary-overview-text") })
	end
	for t, topic in s.topics do
		local children: { Node } = { Header(topic.heading, `topic-{t}-heading`) }
		for b, bullet in topic.bullets do
			table.insert(children, Bullet(bullet, `topic-{t}-bullet-{b}`))
		end
		add(`topic-{t}`, children)
	end
	if #s.next_steps > 0 then
		local children: { Node } = { Header("Next steps") }
		for _, step in s.next_steps do
			table.insert(children, NextStep(step))
		end
		add("summary-next-steps", children)
	end
	if #s.decisions > 0 then
		local children: { Node } = { Header("Decisions") }
		for d, decision in s.decisions do
			table.insert(children, Bullet(decision, `decision-{d}`))
		end
		add("summary-decisions", children)
	end
	return ui.scroll {
		id = "summary",
		w_full = true,
		grow = 1,
		basis = 0,
		ui.column { w_full = true, px = 16, pt = 8, pb = 16, sections },
	}
end

-- No summary and nothing processing: write one.
local function Empty(): Node
	return ui.column {
		id = "summary-empty",
		w_full = true,
		grow = 1,
		items = "center",
		justify = "center",
		gap = 12,
		ui.text { "No summary yet", text_size = 13, color = "$rc.text.secondary" },
		ui.button {
			id = "regenerate",
			h = 28,
			px = 10,
			gap = 6,
			items = "center",
			rounded = 6,
			bg = "$rc.fill.selected",
			hover = { bg = "$rc.fill.selected" },
			on_click = actions.regenerate_summary,
			Icon { name = "retry", size = 14, color = "$rc.text.primary" },
			ui.text { "Write summary", text_size = 13, weight = 500, color = "$rc.text.primary" },
		},
	}
end

return function(): Node
	local shown = ui.computed(function(): string
		if host.summary:get().id ~= "" then
			return "summary"
		end
		return if processing.active(host.processing:get()) then "waiting" else "empty"
	end)
	-- A new summary (another meeting, or written again) is new content: its
	-- sections fade in again.
	local current = ui.computed(function(): { host.Summary }
		local s = host.summary:get()
		return if s.id ~= "" then { s } else {}
	end)
	return ui.column {
		id = "summary-tab",
		w_full = true,
		grow = 1,
		Processing(),
		ui.switch(shown, {
			summary = ui.each(current, Content, {
				-- Ticks don't rebuild it (they read the state); a new summary does.
				key = function(s: host.Summary): string
					local key = s.id .. "|" .. s.overview .. "|" .. #s.next_steps
					for _, step in s.next_steps do
						key ..= "|" .. step.text
					end
					return key
				end,
				w_full = true,
				grow = 1,
				basis = 0,
			}),
			waiting = ui.div { id = "summary-waiting" },
			empty = Empty(),
		}),
	}
end
