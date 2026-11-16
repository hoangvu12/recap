--!strict
-- Home's list column: "Start new notes", then the meetings under their day.
local actions = require("@app/actions")
local host = require("@app/host")
local meetings = require("@lib/meetings")
local format = require("@lib/format")
local Icon = require("@components/icon")
local Keycap = require("@components/keycap")

export type Props = { shown: ReadOnlyState<{ host.MeetingItem }>, selected: ReadOnlyState<string> }

-- The Start notes hotkey as Settings has it, as keycaps; none when cleared.
local function StartKeys(): Node
	local keys = function(): string
		return host.settings:get().hotkeys.start_notes
	end
	return ui.show(
		function()
			return keys() ~= ""
		end,
		ui.row {
			id = "start-notes-keys",
			gap = 2,
			shrink = 0,
			ui.each(
				ui.computed(function(): { string }
					return string.split(keys(), "+")
				end),
				function(key: string)
					return Keycap(key)
				end
			),
			-- The whole hotkey, for tests and assistive technology; not drawn.
			ui.text { keys, id = "start-notes-hotkey", w = 0, h = 0, overflow = "clip", opacity = 0 },
		}
	)
end

local function StartRow(): Node
	return ui.button {
		id = "start-notes",
		h = 38,
		w_full = true,
		shrink = 0,
		px = 8,
		gap = 10,
		items = "center",
		rounded = 8,
		bg = "$rc.transparent",
		hover = { bg = "$rc.fill.hover" },
		on_click = actions.start_notes,
		ui.row { w = 18, h = 18, shrink = 0, items = "center", Icon { name = "record", color = "$rc.rec" } },
		ui.text { "Start new notes", grow = 1, basis = 0, text_size = 13, weight = 500, color = "$rc.text.primary" },
		StartKeys(),
	}
end

local function MeetingRow(selected: ReadOnlyState<string>, item: State<host.MeetingItem>): Node
	local first = item:peek()
	local id = first.id
	local is_selected = function()
		return selected:get() == id
	end
	-- Hover tints the button; the selection is a fill inside it (a hover
	-- style can't follow state).
	return ui.button {
		id = `meeting-{id}`,
		role = "option",
		selected = is_selected,
		h = 38,
		w_full = true,
		shrink = 0,
		p = 0,
		rounded = 8,
		bg = "$rc.transparent",
		hover = { bg = "$rc.fill.hover" },
		on_click = function()
			actions.select_meeting(id)
		end,
		on_double_click = function()
			actions.select_meeting(id)
			actions.open_meeting(id)
		end,
		ui.row {
			w_full = true,
			h_full = true,
			px = 8,
			gap = 10,
			items = "center",
			rounded = 8,
			bg = function()
				return if is_selected() then "$rc.fill.selected" else "$rc.transparent"
			end,
		ui.row {
			w = 18,
			h = 18,
			shrink = 0,
			items = "center",
			Icon { name = if meetings.unfinished(first) then "warning" else "meeting" },
		},
		ui.text {
			function()
				return meetings.title(item:get())
			end,
			id = `meeting-{id}-title`,
			literal = true,
			grow = 1,
			basis = 0,
			truncate = true,
			text_size = 13,
			weight = 500,
			color = "$rc.text.primary",
		},
		ui.text {
			function()
				local m = item:get()
				return if meetings.unfinished(m) then "Recover" else format.duration(m.duration_ms)
			end,
			id = `meeting-{id}-duration`,
			shrink = 0,
			text_size = 13,
			color = "$rc.text.secondary",
			tabular = true,
		},
		},
	}
end

local function Section(selected: ReadOnlyState<string>, section: State<meetings.Section>, index: number): Node
	return ui.column {
		w_full = true,
		ui.row {
			h = 32,
			w_full = true,
			px = 8,
			pb = 6,
			items = "end",
			ui.text {
				function()
					return section:get().label
				end,
				id = `day-{index}`,
				text_size = 11,
				weight = 500,
				color = "$rc.text.secondary",
			},
		},
		ui.each(
			ui.computed(function()
				return section:get().items
			end),
			function(_, _, item)
				return MeetingRow(selected, item)
			end,
			{
				key = function(item: host.MeetingItem): string
					return item.id
				end,
				w_full = true,
			}
		),
	}
end

return function(props: Props): Node
	-- Days are relative to now; the list rebuilds when meetings change.
	local sections = ui.computed(function()
		return meetings.sections(props.shown:get(), os.time() * 1000)
	end)
	return ui.scroll {
		id = "home-list",
		w = 300,
		shrink = 0,
		h_full = true,
		ui.column {
			w_full = true,
			px = 8,
			py = 4,
			StartRow(),
			ui.each(sections, function(_, index, section)
				return Section(props.selected, section, index)
			end, {
				key = function(section: meetings.Section): string
					return section.label
				end,
				w_full = true,
			}),
		},
	}
end
