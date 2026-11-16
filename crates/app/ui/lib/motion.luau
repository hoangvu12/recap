--!strict
-- The motion tokens (motion_spec §2.0, DESIGN §9.4) and the one way to play
-- them. Every animation in the app goes through here, so reduced motion is
-- honoured in one place.
--
-- Everything here is a BINDING (`ui.tween`, `ui.loop`), which each window
-- plays on its own frames. Recap must not use `anime.animate`, nor
-- `ui.presence`/`ui.each` enter/exit: zgx ticks the anime engine on the
-- root window's frames, and our root window is hidden, so those never play
-- (a node stays at its first frame, opacity 0, and a leaving one never
-- leaves). ADR 0017.
--
--   motion.Enter(kind, props, m?)     a node (`"column"`, `"row"`, `"div"`)
--                                     that plays `m` (default ENTER) as it mounts
--   motion.Presence(cond, build, o)   shows `build()` while `cond()`; fades out
--   motion.entrance(play, m?)         ENTER as props, inside a ui.component
--   motion.tween / motion.color       a property easing as a state changes
--   motion.Looping(moving, still)     a loop, or its still form
--   motion.reduced()                  tracked: is motion reduced now?
--
-- Only opacity, colour and transform animate; never layout or blur.
--
-- Reduced motion: Windows' "Animation effects" off (zgx reads
-- SPI_GETCLIENTAREAANIMATION into `ui.prefers.reduced_motion`, live), or the
-- `reduce_motion` state set (tests; a setting of our own later). Then every
-- transform is dropped, every duration is 0 except opacity fades of 150 ms or
-- more, which become a 100 ms fade, and loops stand still.

local M = {}

-- Durations, ms.
M.instant = 0
M.fast_in = 100
M.fast_out = 200
M.base = 150
M.enter_ms = 200
M.reveal = 240
M.scroll = 200
-- The calm wash after a rename (motion_spec §2.7).
M.wash = 600
-- Every entrance's offset, px.
M.RISE = 4

M.ease = {
	out = "cubicBezier(0, 0, .2, 1)",
	-- Raycast's ease-out-expo.
	outExpo = "cubicBezier(.16, 1, .3, 1)",
	-- Raycast's switch thumb: a slight overshoot (DESIGN §9.2).
	switch = "cubicBezier(.16, -.12, .29, 1.31)",
}
-- The switch thumb's slide.
M.switch_ms = 366

-- Reduced motion's fade: what a fade of 150 ms or more becomes.
M.REDUCED_FADE = 100

-- A motion: opacity from → to, and transforms that start at `y` (px) and
-- `scale` and end at 0 and 1; over `duration` ms after `delay`.
export type Motion = {
	opacity: { number }?,
	y: number?,
	scale: number?,
	duration: number,
	delay: number?,
	ease: string?,
}

-- A new thing: fade in + 4 px rise, 200 ms ease-out-expo (`motion.enter`).
M.ENTER = { opacity = { 0, 1 }, y = M.RISE, duration = M.enter_ms, ease = M.ease.outExpo } :: Motion
-- A fade in only (`motion.base`, 150 ms ease-out).
M.FADE = { opacity = { 0, 1 }, duration = M.base, ease = M.ease.out } :: Motion
-- A popover: fade + scale .98 → 1, 150 ms ease-out-expo.
M.POP = { opacity = { 0, 1 }, scale = 0.98, duration = M.base, ease = M.ease.outExpo } :: Motion

-- Forces reduced motion on (tests, or our own setting). (`ui` is nil when a
-- test file requires this module for its pure parts.)
local forced: any = if ui then ui.state("reduce_motion", false) else nil
M.forced = forced

-- Is motion reduced now? Tracked: a computed or effect reading it follows
-- the system setting as the user changes it.
function M.reduced(): boolean
	if forced and forced:get() then
		return true
	end
	local prefers = (ui :: any).prefers
	return prefers ~= nil and prefers.reduced_motion:get() == true
end

-- Reduced now, untracked (for what is decided as a node is built).
local function reduced_now(): boolean
	if forced and forced:peek() then
		return true
	end
	local prefers = (ui :: any).prefers
	return prefers ~= nil and prefers.reduced_motion:peek() == true
end

-- `m` as it plays, reduced or not: nil when nothing plays (the node is
-- simply at its end). Pure.
function M.plan(m: Motion, reduced: boolean): Motion?
	if not reduced then
		return m
	end
	if m.opacity == nil or m.duration < M.base then
		return nil
	end
	return { opacity = m.opacity, duration = M.REDUCED_FADE, delay = 0, ease = M.ease.out }
end

-- `props` with `extra`'s fields added.
function M.with(props: { [any]: any }, extra: { [string]: any }): { [any]: any }
	for k, v in extra do
		props[k] = v
	end
	return props
end

-- `m` as props (`opacity`, `y`, `scale` bindings) that play once, from now.
-- Call it inside a `ui.component` (it holds state).
function M.bind(m: Motion): { [string]: any }
	local p = M.plan(m, reduced_now())
	if p == nil then
		return {}
	end
	local played = ui.use_state(false)
	ui.after(math.max(p.delay or 0, 1), function()
		played:set(true)
	end)
	local function tween(from: number, to: number): any
		return ui.tween(played, { from = from, to = to, duration = math.max(p.duration, 1), ease = p.ease or M.ease.out })
	end
	local out: { [string]: any } = {}
	if p.opacity then
		out.opacity = tween(p.opacity[1], p.opacity[2])
	end
	if p.y then
		out.y = tween(p.y, 0)
	end
	if p.scale then
		out.scale = tween(p.scale, 1)
	end
	return out
end

-- The ENTER motion (or `m`) as props when `play`, else none. Inside a
-- `ui.component`.
function M.entrance(play: boolean, m: Motion?): { [string]: any }
	if not play then
		return {}
	end
	return M.bind(m or M.ENTER)
end

type EnterProps = { kind: string, props: { [any]: any }, motion: Motion }

local EnterNode = if ui
	then ui.component(function(p: EnterProps): Node
		local make = (ui :: any)[p.kind]
		return make(M.with(table.clone(p.props), M.bind(p.motion)))
	end)
	else nil

-- A `ui.<kind>` node built from `props` that plays `m` (ENTER by default)
-- each time it mounts.
function M.Enter(kind: string, props: { [any]: any }, m: Motion?): Node
	return (EnterNode :: any) { kind = kind, props = props, motion = m or M.ENTER }
end

export type PresenceOptions = {
	-- The fade out, ms (100 by default); reduced motion: 100 ms when it is
	-- 150 or more, else none.
	exit: number?,
	-- Props of the box that holds it (absolute insets, sizing).
	box: { [string]: any }?,
}

type PresenceProps = { cond: () -> boolean, build: () -> Node, opts: PresenceOptions }

local PresenceNode = if ui
	then ui.component(function(p: PresenceProps): Node
		local exit = p.opts.exit or M.fast_in
		local mounted = ui.use_state(false)
		local leaving = ui.use_state(false)
		local seq = 0
		ui.effect(function()
			local on = p.cond()
			seq += 1
			if on then
				leaving:set(false)
				mounted:set(true)
				return
			end
			if not mounted:peek() then
				return
			end
			local ms = exit
			if reduced_now() then
				ms = if ms >= M.base then M.REDUCED_FADE else 0
			end
			if ms <= 0 then
				mounted:set(false)
				return
			end
			leaving:set(true)
			local mine = seq
			ui.after(ms, function()
				if mine == seq then
					mounted:set(false)
					leaving:set(false)
				end
			end)
		end)
		local fade = if reduced_now() then M.REDUCED_FADE else exit
		local box: { [any]: any } = table.clone(p.opts.box or { w_full = true })
		box.opacity = ui.tween(leaving, { from = 1, to = 0, duration = math.max(fade, 1), ease = M.ease.out })
		-- A new mount builds its content again (its entrance plays again).
		table.insert(
			box,
			ui.each(
				ui.computed(function(): { number }
					return if mounted:get() then { 1 } else {}
				end),
				function(_)
					return p.build()
				end
			)
		)
		return ui.column(box)
	end)
	else nil

-- Shows `build()` while `cond()` holds; as it stops, the content fades out
-- (`opts.exit` ms) before it goes. Entrances belong in `build` (Enter).
function M.Presence(cond: () -> boolean, build: () -> Node, opts: PresenceOptions?): Node
	return (PresenceNode :: any) { cond = cond, build = build, opts = opts or {} }
end

export type TweenOptions = { duration: number?, ease: string?, from: any?, to: any? }

-- A property that eases as `source` (a state) changes: `ui.tween` with our
-- defaults (150 ms ease-out). Colours may be tokens (`"$rc.text.primary"`):
-- zgx mixes theme colours. Under reduced motion (as the node is built) it
-- changes at once.
function M.tween(source: any, opts: TweenOptions?): any
	local o: { [string]: any } = {}
	for k, v in (opts or {}) :: any do
		o[k] = v
	end
	o.duration = if reduced_now() then 1 else (o.duration or M.base)
	o.ease = o.ease or M.ease.out
	return ui.tween(source, o)
end

-- A colour that eases to the token `token()` names whenever it changes
-- (`motion.fast` colour swaps, 150 ms by default).
function M.color(token: () -> string, duration: number?): any
	return M.tween(ui.computed(token), { duration = duration or M.base })
end

-- A loop (`moving`, built with ui.loop) while motion is allowed, `still`
-- when it is reduced; it swaps as the setting changes.
function M.Looping(moving: Node, still: Node): Node
	return ui.show(function()
		return not M.reduced()
	end, moving, still)
end

return M
