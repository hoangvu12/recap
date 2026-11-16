--!strict
-- The motion tokens (motion_spec §2.0, DESIGN §9.4) and the one way to play
-- them. Every animation in the app goes through here, so reduced motion is
-- honoured in one place:
--
--   motion.enter(el)            a new thing: opacity 0→1 + 4 px rise, 200 ms
--   motion.fade(el, ms?)        opacity 0→1 only (150 ms)
--   motion.pop(el)              a popover: opacity + scale .98→1, 150 ms
--   motion.animate(el, params)  anything else, anime.js parameters
--   motion.Looping(moving, still)   a loop, or its still form
--   motion.reduced()            tracked: is motion reduced now?
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
}

-- Reduced motion's fade: what a fade of 150 ms or more becomes.
M.REDUCED_FADE = 100

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

local TRANSFORMS = { "x", "y", "scale", "scaleX", "scaleY", "rotate", "translateX", "translateY" }

-- `params` as reduced motion plays it: nil when nothing should play.
function M.reduce(params: { [string]: any }): { [string]: any }?
	if params.opacity == nil then
		return nil
	end
	local duration = params.duration or 0
	if duration < M.base then
		return nil
	end
	local out: { [string]: any } = {}
	for k, v in params do
		out[k] = v
	end
	for _, k in TRANSFORMS do
		out[k] = nil
	end
	out.duration = M.REDUCED_FADE
	out.delay = 0
	out.ease = M.ease.out
	return out
end

-- Play anime.js `params` on `el`, as reduced motion allows.
function M.animate(el: any, params: { [string]: any })
	local played: { [string]: any }? = params
	if M.reduced() then
		played = M.reduce(params)
	end
	if played then
		anime.animate(el, played)
	end
end

export type Options = { duration: number?, delay: number?, rise: number?, ease: string? }

-- A new element arrives: fades in and rises 4 px (`motion.enter` 200 ms
-- ease-out-expo). Use as `on_mount = motion.enter`, or call it from one.
function M.enter(el: any, opts: Options?)
	local o: Options = opts or {}
	M.animate(el, {
		opacity = { 0, 1 },
		y = { o.rise or M.RISE, 0 },
		duration = o.duration or M.enter_ms,
		delay = o.delay or 0,
		ease = o.ease or M.ease.outExpo,
	})
end

-- Fades in only (`motion.base` 150 ms ease-out).
function M.fade(el: any, duration: number?)
	M.animate(el, { opacity = { 0, 1 }, duration = duration or M.base, ease = M.ease.out })
end

-- A popover opens: opacity + scale .98→1, 150 ms ease-out-expo.
function M.pop(el: any)
	M.animate(el, { opacity = { 0, 1 }, scale = { 0.98, 1 }, duration = M.base, ease = M.ease.outExpo })
end

-- `on_mount` handlers for the above, for props.
function M.on_enter(opts: Options?): (any) -> ()
	return function(el: any)
		M.enter(el, opts)
	end
end

function M.on_fade(duration: number?): (any) -> ()
	return function(el: any)
		M.fade(el, duration)
	end
end

-- A loop (`moving`, built with ui.loop) while motion is allowed, `still`
-- when it is reduced; it swaps as the setting changes.
function M.Looping(moving: Node, still: Node): Node
	return ui.show(function()
		return not M.reduced()
	end, moving, still)
end

return M
