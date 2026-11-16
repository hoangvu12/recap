--!strict
-- The caption overlay (screens/1): always dark, the rec dot and the timer at
-- the top left, then the last lines of what Them said, newest at the bottom.
-- The newest line's final words are primary and its partial at fg 55%, in
-- one paragraph (ADR 0001); earlier lines are secondary, as are my own.
--
-- States (motion_spec §2.5):
-- - live;
-- - reconnecting: "Reconnecting" over the last line, dimmed;
-- - error: what failed, and "Switch to <engine>" (recording goes on);
-- - paused: the last line dimmed, then "Paused" and the resume hotkey.
-- No new words for 8 s: the text dims (400 ms), undimmed at once by the
-- next. A new turn or a wrap shifts the lines up (150 ms); a partial
-- revised in place doesn't. Hovering 150 ms shows the toolbar.
--
-- Caption size, lines, partial text and my lines come from Settings, live.
local host = require("@app/host")
local state = require("@app/state")
local actions = require("@app/actions")
local theme = require("@theme")
local format = require("@lib/format")
local RecDot = require("@components/rec_dot")
local Icon = require("@components/icon")
local Keycap = require("@components/keycap")

-- The partial's colour: fg 55% on the dark glass.
local PARTIAL = "#FFFFFF8C"
local IDLE_MS = 8000
local IDLE_OPACITY = 0.5

local function reduced(): boolean
	local prefers = (ui :: any).prefers
	return prefers ~= nil and prefers.reduced_motion:get() == true
end

local function text_size(): number
	return host.settings:get().caption_size
end

local function line_height(): number
	return math.floor(text_size() * 1.4 + 0.5)
end

type Mode = "live" | "reconnecting" | "error" | "paused"

local function mode(): Mode
	local live = host.live:get()
	if live.status == "error" then
		return "error"
	elseif host.meeting:get().state == "paused" then
		return "paused"
	elseif live.status == "reconnecting" then
		return "reconnecting"
	end
	return "live"
end

-- The lines shown, oldest first: as many as Settings says, less one for a
-- status line; with partial text off, a turn still all partial waits.
local shown = ui.computed(function(): { host.CaptionLine }
	local m = mode()
	if m == "error" then
		return {}
	end
	local s = host.settings:get()
	local want = math.clamp(s.caption_lines, 1, 3)
	if m ~= "live" then
		want -= 1
	end
	local lines = {}
	for _, l in host.captions:get() do
		if s.partial_text or l.final ~= "" then
			table.insert(lines, l)
		end
	end
	local out = {}
	for i = math.max(1, #lines - want + 1), #lines do
		table.insert(out, lines[i])
	end
	return out
end)

-- Line `back` from the newest (0 = newest).
local function line(back: number): Node
	local at = function(): host.CaptionLine?
		local lines = shown:get()
		return lines[#lines - back]
	end
	local dimmed = function(): boolean
		return mode() ~= "live"
	end
	return ui.show(
		function()
			return at() ~= nil
		end,
		ui.column {
			w_full = true,
			-- My line (my captions on): dimmed. Tests read this marker.
			ui.show(function()
				local l = at()
				return l ~= nil and l.source == "me"
			end, ui.div { id = `caption-{back}-me`, w = 0, h = 0, aria_hidden = true }),
			ui.rich {
				id = `caption-{back}`,
				class = "caption",
				wrap = true,
				w_full = true,
				text_size = text_size,
				line_height = line_height,
				weight = 500,
				ui.span {
					function()
						local l = at()
						return if l then l.final else ""
					end,
					color = function()
						local l = at()
						if dimmed() or (back == 2 and #shown:get() == 3) then
							return "$rc.text.tertiary"
						end
						return if back == 0 and l and l.source == "them" then "$rc.text.primary" else "$rc.text.secondary"
					end,
				},
				ui.span {
					function()
						local l = at()
						if l == nil or l.partial == "" or not host.settings:get().partial_text then
							return ""
						end
						return if l.final == "" then l.partial else " " .. l.partial
					end,
					color = function()
						return if dimmed() then "$rc.text.tertiary" else PARTIAL
					end,
				},
			},
		}
	)
end

local function reconnecting(): Node
	return ui.row {
		gap = 6,
		items = "center",
		h = 22,
		ui.div {
			w = 16,
			h = 16,
			rotate = ui.loop { from = 0, to = 360, duration = 800, ease = "linear", yoyo = false },
			Icon { name = "spinner", color = "$rc.text.tertiary" },
		},
		ui.text { "Reconnecting", id = "overlay-reconnecting", text_size = 16, color = "$rc.text.tertiary" },
	}
end

local function paused(): Node
	return ui.row {
		gap = 10,
		items = "center",
		h = line_height,
		Icon { name = "pause", size = 16, color = "$rc.text.primary" },
		ui.text { "Paused", id = "overlay-paused", text_size = text_size, weight = 500, color = "$rc.text.primary" },
		ui.presence(function()
			return host.settings:get().hotkeys.pause
		end, function(keys: string): Node
			local caps: { [any]: any } = { gap = 4, items = "center" }
			local i = 0
			for key in string.gmatch(keys, "[^+]+") do
				i += 1
				table.insert(caps, Keycap(key, `overlay-paused-key-{i}`))
			end
			return ui.row(caps)
		end),
	}
end

local function failed(): Node
	return ui.column {
		gap = 4,
		ui.row {
			gap = 8,
			items = "center",
			Icon { name = "error-circle", size = 16, color = "$rc.red" },
			ui.text {
				function()
					return host.live:get().error
				end,
				id = "overlay-error",
				text_size = 16,
				weight = 500,
				color = "$rc.text.primary",
			},
		},
		ui.show(
			function()
				return host.live:get().switch_to ~= nil
			end,
			ui.button {
				id = "switch-live",
				on_click = function()
					local choice = host.live:peek().switch_to
					if choice then
						actions.switch_live(choice)
					end
				end,
				ui.text {
					function()
						local choice = host.live:get().switch_to
						return if choice then `Switch to {choice.name}` else ""
					end,
					text_size = 16,
					color = "$rc.accent",
				},
			}
		),
	}
end

-- A 28 px toolbar button; `on` (if given) shows it switched on.
local function tool(id: string, icon: string | Node, label: string, on_click: () -> (), on: (() -> boolean)?): Node
	local tint = function()
		return if on and on() then "$rc.text.primary" else "$rc.text.secondary"
	end
	return ui.button {
		id = id,
		label = label,
		w = 28,
		h = 28,
		rounded = 6,
		items = "center",
		justify = "center",
		bg = function()
			return if on and on() then "$rc.fill.selected" else "$rc.transparent"
		end,
		hover = { bg = "$rc.fill.hover" },
		on_click = on_click,
		if type(icon) == "string" then Icon { name = icon, color = tint :: any } else icon,
		if on
			then ui.show(on, ui.div { id = `{id}-on`, absolute = true, w = 0, h = 0, aria_hidden = true })
			else nil,
	}
end

local function toolbar(): Node
	return ui.row {
		id = "overlay-toolbar",
		gap = 2,
		p = 4,
		rounded = 8,
		bg = "$rc.bg.card",
		border = 1,
		border_color = "$rc.stroke.hairline",
		shadow = { x = 0, y = 4, blur = 12, color = "$rc.shadow.pop" },
		tool(
			"tb-pause",
			ui.show(function()
				return host.meeting:get().state == "paused"
			end, Icon { name = "play" }, Icon { name = "pause" }),
			"Pause",
			actions.toggle_pause
		),
		tool("tb-stop", "stop", "Stop", actions.stop),
		tool("tb-notes", "notes-panel", "Open notes", actions.open_notes),
		tool("tb-smaller", "text-smaller", "Smaller text", function()
			actions.caption_size(-2)
		end),
		tool("tb-larger", "text-larger", "Larger text", function()
			actions.caption_size(2)
		end),
		tool("tb-my-lines", "my-lines", "Show my lines", actions.toggle_my_lines, function()
			return host.settings:get().caption_my_speech
		end),
		tool("tb-pin", "pin", "Keep on top", actions.toggle_pinned, function()
			return host.overlay:get().pinned
		end),
		tool("tb-click-through", "click-through", "Click through", actions.toggle_click_through, function()
			return host.overlay:get().click_through
		end),
	}
end

return function(): Node
	-- Dim after 8 s without new words; new words undim at once.
	ui.effect(function()
		host.captions:get()
		state.captions_idle:set(false)
		ui.after(IDLE_MS, function()
			state.captions_idle:set(true)
		end)
	end)
	ui.effect(function()
		if state.captions_idle:get() then
			anime.animate("#captions-block", {
				opacity = IDLE_OPACITY,
				duration = if reduced() then 100 else 400,
				ease = "out(2)",
			})
		else
			anime.utils.set("#captions-block", { opacity = 1 })
		end
	end)

	-- Shift the block up: a new turn (one line), or the bottom line
	-- wrapping (what it grew). Never for a partial revised in place.
	local function shift(px: number)
		local s = state.caption_shift:peek()
		state.caption_shift:set({ px = px, count = s.count + 1 })
		if not reduced() then
			anime.animate("#captions-block", { y = { px, 0 }, duration = 150, ease = "outExpo" })
		end
	end
	local newest: number? = nil
	local turned = false
	local height: number? = nil
	ui.effect(function()
		local lines = shown:get()
		local last = lines[#lines]
		local index = if last then last.index else nil
		if index ~= nil and newest ~= nil and index ~= newest then
			turned = true
			shift(line_height())
			ui.after(100, function()
				turned = false
			end)
		end
		newest = index
	end)

	-- Hovering 150 ms shows the toolbar; leaving hides it.
	local stop_hover: (() -> ())? = nil
	local function hovered(inside: boolean)
		if stop_hover then
			stop_hover()
			stop_hover = nil
		end
		if inside then
			stop_hover = ui.after(150, function()
				state.overlay_toolbar:set(true)
			end)
		else
			state.overlay_toolbar:set(false)
		end
	end
	local quick = reduced()

	return ui.column {
		id = "overlay-window",
		theme = theme.DARK,
		relative = true,
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
		on_hover = hovered,
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
			-- Clicks go through: say so where the eye already is.
			ui.show(function()
				return host.overlay:get().click_through
			end, ui.div {
				id = "overlay-click-through",
				Icon { name = "click-through", size = 12, color = "$rc.text.tertiary" },
			}),
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
			ui.column {
				id = "captions-block",
				w_full = true,
				shrink = 0,
				on_resize = function(size)
					local before = height
					height = size.h
					if turned or before == nil then
						return
					end
					if size.h > before + 2 then
						shift(size.h - before)
					end
				end,
				ui.show(function()
					return mode() == "reconnecting"
				end, reconnecting()),
				line(2),
				line(1),
				line(0),
				ui.show(function()
					return mode() == "paused"
				end, paused()),
				ui.show(function()
					return mode() == "error"
				end, failed()),
			},
		},
		-- Markers tests read: idle-dimmed, and dimmed by a status.
		ui.show(state.captions_idle, ui.div { id = "captions-idle", w = 0, h = 0, aria_hidden = true }),
		ui.show(function()
			local m = mode()
			return m == "reconnecting" or m == "paused"
		end, ui.div { id = "captions-dimmed", w = 0, h = 0, aria_hidden = true }),
		ui.presence(function()
			return if state.overlay_toolbar:get() then "on" else nil
		end, function()
			return toolbar()
		end, {
			absolute = true,
			top = 8,
			right = 8,
			enter = { opacity = { 0, 1 }, y = { if quick then 0 else 4, 0 }, duration = if quick then 0 else 100, ease = "out(2)" },
			exit = { opacity = 0, duration = if quick then 0 else 200, ease = "out(2)" },
		}),
	}
end
