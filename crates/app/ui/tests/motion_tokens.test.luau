--!nonstrict
-- lib/motion's reduced-motion rule (motion_spec §2.0): no transforms, fades
-- of 150 ms or more become 100 ms, everything else doesn't play.
local motion = require("../lib/motion")

test("the motion tokens are the spec's", function()
	expect(motion.instant).to_be(0)
	expect(motion.fast_in).to_be(100)
	expect(motion.fast_out).to_be(200)
	expect(motion.base).to_be(150)
	expect(motion.enter_ms).to_be(200)
	expect(motion.reveal).to_be(240)
	expect(motion.RISE).to_be(4)
	expect(motion.ease.outExpo).to_be("cubicBezier(.16, 1, .3, 1)")
end)

test("reduced: an entrance keeps its fade, 100 ms, without the rise or the delay", function()
	local r = motion.reduce({ opacity = { 0, 1 }, y = { 4, 0 }, scale = { 0.98, 1 }, duration = 240, delay = 160 })
	expect(r).never.to_be_nil()
	expect(r.y).to_be_nil()
	expect(r.scale).to_be_nil()
	expect(r.duration).to_be(100)
	expect(r.delay).to_be(0)
	expect(r.opacity).to_equal({ 0, 1 })
end)

test("reduced: a short fade or a move alone doesn't play", function()
	expect(motion.reduce({ opacity = { 1, 0 }, duration = 100 })).to_be_nil()
	expect(motion.reduce({ y = { 4, 0 }, duration = 200 })).to_be_nil()
end)
