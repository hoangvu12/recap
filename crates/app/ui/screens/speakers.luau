--!strict
-- The Name speakers dialog (screens/6, Wispr's): each speaker with its
-- chip, two things it said, a ▶ clip of its voice and its name, picked from
-- the known people or typed; the AI's hint is a chip under an empty field.
-- Me is Settings' name, locked. Skip Esc, Save names Ctrl ↵. It fades in
-- (200 ms, 4 px rise; motion_spec §2.9).
local host = require("@app/host")
local actions = require("@app/actions")
local Chip = require("@components/chip")
local Icon = require("@components/icon")
local Keycap = require("@components/keycap")
local motion = require("@lib/motion")

local HUES = { "purple", "green", "orange", "blue", "yellow" }

local function hue(n: number): string
	return HUES[(math.max(n, 1) - 1) % #HUES + 1]
end

-- What a speaker's field holds while the dialog is open.
local function field(id: string): State<string>
	return ui.state(`dialog_name.{id}`, "")
end

-- ▶, or ■ while its clip plays.
local function Play(i: number, s: host.DialogSpeaker, playing: () -> boolean): Node
	local h = hue(s.n)
	return ui.button {
		id = `speaker-{i}-play`,
		label = `Play {s.label}`,
		w = 40,
		h = 40,
		shrink = 0,
		rounded = 999,
		items = "center",
		justify = "center",
		bg = `$rc.{h}.soft`,
		hover = { bg = `$rc.{h}.soft` },
		on_click = function()
			actions.play_speaker(s.id)
		end,
		ui.switch(function()
			return if playing() then "stop" else "play"
		end, {
			play = Icon { name = "play", size = 16, color = `$rc.{h}`, id = `speaker-{i}-play-icon` },
			stop = Icon { name = "stop", size = 16, color = `$rc.{h}`, id = `speaker-{i}-stop-icon` },
		}),
	}
end

-- The 2 px line that crosses as the clip plays (scaleX, linear). It tells
-- playback's progress, so it runs under reduced motion too. A binding, not
-- anime: anime doesn't play in Recap's windows (lib/motion).
local Line = ui.component(function(props: { i: number }): Node
	local started = ui.use_state(false)
	ui.after(1, function()
		started:set(true)
	end)
	return ui.div {
		id = `speaker-{props.i}-progress`,
		w_full = true,
		h = 2,
		rounded = 1,
		bg = "$rc.accent",
		origin = { 0, 0.5 },
		scale_x = ui.tween(started, {
			from = 0,
			to = 1,
			duration = math.max(host.playback:peek().duration_ms, 1),
			ease = "linear",
		}),
	}
end)

local function Progress(i: number, playing: () -> boolean): Node
	return ui.show(playing, Line { i = i })
end

-- The name: Me's locked; the others a field with the people to pick from.
local function Name(i: number, s: host.DialogSpeaker, people: { string }): Node
	local function Box(children: { Node }): Node
		return ui.row {
			w_full = true,
			h = 34,
			pl = 12,
			pr = 6,
			gap = 6,
			items = "center",
			rounded = 6,
			bg = "$rc.fill.hover",
			border = 1,
			border_color = "$rc.stroke.field",
			children,
		}
	end
	if s.locked then
		return Box {
			ui.text {
				if s.name ~= "" then s.name else "Me",
				id = `speaker-{i}-name`,
				literal = true,
				grow = 1,
				basis = 0,
				truncate = true,
				text_size = 13,
				color = "$rc.text.primary",
			},
			Icon { name = "lock", size = 14, color = "$rc.text.tertiary", id = `speaker-{i}-locked` },
			Icon { name = "chevrons-up-down", size = 14, color = "$rc.text.tertiary" },
		}
	end
	local text = field(s.id)
	local open = ui.state(`dialog_menu.{s.id}`, false)
	local items: { Node } = {}
	for p, person in people do
		table.insert(
			items,
			ui.menu_item {
				id = `speaker-{i}-person-{p}`,
				label = person,
				on_click = function()
					text:set(person)
				end,
				ui.text { person, literal = true, text_size = 13 },
			}
		)
	end
	return Box {
		ui.input {
			id = `speaker-{i}-field`,
			value = text,
			label = `Name of {s.label}`,
			placeholder = "Who is this?",
			placeholder_color = "$rc.text.tertiary",
			w = 0,
			grow = 1,
			h = 30,
			p = 0,
			text_size = 13,
			bg = "$rc.transparent",
			border = 0,
		},
		ui.menu {
			open = open,
			label = `People for {s.label}`,
			anchor = ui.button {
				id = `speaker-{i}-people`,
				w = 24,
				h = 24,
				p = 0,
				items = "center",
				justify = "center",
				rounded = 4,
				bg = "$rc.transparent",
				hover = { bg = "$rc.fill.hover" },
				on_click = ui.toggle(open),
				Icon { name = "chevrons-up-down", size = 14, color = "$rc.text.tertiary" },
			},
			build = function(): Node
				return ui.column { w = 200, items }
			end,
		},
	}
end

-- The AI's hint ("+ David"), while the field is empty.
local function Suggest(i: number, s: host.DialogSpeaker): Node?
	if s.locked or s.suggest == "" then
		return nil
	end
	local text = field(s.id)
	return ui.show(
		function()
			return string.match(text:get(), "%S") == nil
		end,
		ui.button {
			id = `speaker-{i}-suggest`,
			h = 26,
			px = 10,
			gap = 4,
			items = "center",
			rounded = 999,
			bg = "$rc.transparent",
			border = 1,
			border_color = "$rc.stroke.field",
			hover = { bg = "$rc.fill.hover" },
			on_click = function()
				text:set(s.suggest)
			end,
			Icon { name = "plus", size = 14, color = "$rc.text.primary" },
			ui.text { s.suggest, literal = true, text_size = 13, color = "$rc.text.primary" },
		}
	)
end

local function Card(i: number, s: host.DialogSpeaker, people: { string }): Node
	local playing = function(): boolean
		return host.playback:get().key == `speaker-{s.id}`
	end
	local quotes: { Node } = {}
	for q, quote in s.quotes do
		table.insert(quotes, ui.text {
			`“{quote}”`,
			id = `speaker-{i}-quote-{q}`,
			literal = true,
			w_full = true,
			wrap = true,
			text_size = 13,
			line_height = 20,
			color = "$rc.text.secondary",
		})
	end
	return ui.column {
		id = `speaker-{i}`,
		w_full = true,
		ui.row {
			w_full = true,
			px = 16,
			py = 16,
			gap = 12,
			items = "start",
			Play(i, s, playing),
			ui.column {
				grow = 1,
				basis = 0,
				gap = 8,
				ui.row { Chip.Speaker(s.label, s.n, `speaker-{i}-chip`) },
				quotes,
				Progress(i, playing),
				Name(i, s, people),
				Suggest(i, s),
			},
		},
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
	}
end

-- What each field holds now: speaker id -> text.
local function typed(d: host.SpeakersDialog): { [string]: string }
	local out: { [string]: string } = {}
	for _, s in d.speakers do
		if not s.locked then
			out[s.id] = field(s.id):peek()
		end
	end
	return out
end

local function save()
	actions.save_names(typed(host.speakers_dialog:peek()))
end

local function bare_button(id: string, label: string, color: string, on_click: () -> (), keys: { Node }): Node
	return ui.button {
		id = id,
		h = 28,
		px = 6,
		gap = 8,
		items = "center",
		rounded = 6,
		bg = "$rc.transparent",
		hover = { bg = "$rc.fill.hover" },
		on_click = on_click,
		ui.text { label, text_size = 13, color = color },
		ui.row { gap = 4, keys },
	}
end

return function(): Node
	local dialogs = ui.computed(function(): { host.SpeakersDialog }
		local d = host.speakers_dialog:get()
		return if d.id ~= "" then { d } else {}
	end)
	-- The dialog comes in: 200 ms fade + 4 px rise (motion_spec §2.9).
	return motion.Enter("column", {
		id = "speakers-window",
		w_full = true,
		h_full = true,
		bg = "$rc.bg.window",
		color = "$rc.text.primary",
		font = "$rc.font.ui",
		on_key = function(e)
			if e.key == "Escape" then
				actions.close_name_speakers()
				return true
			end
			if e.key == "Enter" and e.ctrl then
				save()
				return true
			end
			return false
		end,
		ui.row {
			w_full = true,
			h = 56,
			px = 16,
			shrink = 0,
			items = "center",
			window_drag = true,
			ui.text { "Name speakers", id = "speakers-title", text_size = 18, color = "$rc.text.primary" },
		},
		ui.div { h = 1, w_full = true, shrink = 0, bg = "$rc.stroke.hairline" },
		-- Another meeting's (or new names) is a new list: the fields start
		-- from the names it has.
		ui.each(dialogs, function(d: host.SpeakersDialog)
			local cards: { Node } = {}
			for i, s in d.speakers do
				if not s.locked then
					field(s.id):set(s.name)
				end
				table.insert(cards, Card(i, s, d.people))
			end
			return ui.scroll { id = "speakers-list", w_full = true, grow = 1, basis = 0, ui.column { w_full = true, cards } }
		end, {
			key = function(d: host.SpeakersDialog): string
				local key = d.id
				for _, s in d.speakers do
					key ..= "|" .. s.id .. "=" .. s.name
				end
				return key
			end,
			w_full = true,
			grow = 1,
			basis = 0,
		}),
		ui.div { h = 1, w_full = true, shrink = 0, bg = "$rc.stroke.hairline" },
		ui.row {
			w_full = true,
			h = 42,
			px = 10,
			gap = 4,
			shrink = 0,
			items = "center",
			justify = "end",
			bare_button("skip-names", "Skip", "$rc.text.secondary", actions.close_name_speakers, { Keycap("Esc") }),
			ui.div { w = 1, h = 16, mx = 4, bg = "$rc.stroke.hairline" },
			bare_button("save-names", "Save names", "$rc.text.primary", save, { Keycap("Ctrl"), Keycap("↵") }),
		},
	}, motion.ENTER)
end
