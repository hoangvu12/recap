--!strict
-- The processing card (screens/3a, 3b; DESIGN §9.3 Progress steps): after
-- Stop, three rows of 32 px, a pending circle → a spinner → ✓ with the time
-- it took; a failed step is ✗ in red, with its error, Retry and a way to
-- fix the provider under the rows.
local host = require("@app/host")
local actions = require("@app/actions")
local processing = require("@lib/processing")
local Icon = require("@components/icon")
local motion = require("@lib/motion")

-- The steps the card has, in order (Transcribing, Naming speakers, Writing summary).
local STEPS = 3

local function step(i: number): host.Step?
	return host.processing:get().steps[i]
end

local function StepRow(i: number): Node
	local status = function(): string
		local s = step(i)
		return if s then s.status else "pending"
	end
	return ui.row {
		id = `step-{i}`,
		w_full = true,
		h = 32,
		px = 8,
		gap = 12,
		items = "center",
		-- The icon says the status (ids `step-<i>-<status>`).
		ui.switch(status, {
			pending = Icon { name = "circle-empty", color = "$rc.text.quaternary", id = `step-{i}-pending` },
			-- The spinner fades in (150 ms) and turns every 800 ms; under
			-- reduced motion it stands still.
			running = ui.div {
				id = `step-{i}-running`,
				w = 16,
				h = 16,
				shrink = 0,
				on_mount = motion.on_fade(motion.base),
				motion.Looping(
					ui.div {
						id = `step-{i}-spin`,
						w = 16,
						h = 16,
						rotate = ui.loop { from = 0, to = 360, duration = 800, ease = "linear", yoyo = false },
						Icon { name = "spinner", color = "$rc.accent" },
					},
					Icon { name = "spinner", color = "$rc.accent" }
				),
			},
			-- ✓ crossfades in with a small scale (.8 → 1), no overshoot.
			done = ui.div {
				w = 16,
				h = 16,
				shrink = 0,
				on_mount = function(el)
					motion.animate(el, {
						opacity = { 0, 1 },
						scale = { 0.8, 1 },
						duration = motion.base,
						ease = motion.ease.outExpo,
					})
				end,
				Icon { name = "check", color = "$rc.text.secondary", id = `step-{i}-done` },
			},
			error = ui.div {
				w = 16,
				h = 16,
				shrink = 0,
				on_mount = motion.on_fade(motion.base),
				Icon { name = "error-circle", color = "$rc.red", id = `step-{i}-error` },
			},
		}),
		ui.text {
			function()
				local s = step(i)
				return if s then s.label else ""
			end,
			id = `step-{i}-label`,
			grow = 1,
			basis = 0,
			text_size = 13,
			weight = 500,
			line_height = 16,
			color = function()
				local s = status()
				return if s == "running" or s == "error"
					then "$rc.text.primary"
					elseif s == "done" then "$rc.text.secondary"
					else "$rc.text.tertiary"
			end,
		},
		ui.text {
			function()
				local s = step(i)
				if s == nil then
					return ""
				end
				return if s.status == "done" then processing.took(s.took_ms) elseif s.status == "running" then "…" else ""
			end,
			id = `step-{i}-took`,
			shrink = 0,
			font = "$rc.font.mono",
			text_size = 11,
			color = "$rc.text.tertiary",
			tabular = true,
		},
	}
end

local function Failure(): Node
	return ui.show(
		function()
			return processing.failed(host.processing:get()) ~= nil
		end,
		ui.column {
			id = "processing-error",
			w_full = true,
			px = 12,
			py = 10,
			gap = 10,
			rounded = 6,
			bg = "$rc.fill.hover",
			ui.text {
				function()
					local failed = processing.failed(host.processing:get())
					return if failed then failed.error else ""
				end,
				id = "processing-error-text",
				w_full = true,
				wrap = true,
				text_size = 13,
				line_height = 19,
				color = "$rc.text.primary",
			},
			ui.row {
				gap = 8,
				items = "center",
				ui.button {
					id = "retry",
					h = 28,
					px = 10,
					gap = 6,
					items = "center",
					rounded = 6,
					bg = "$rc.fill.selected",
					hover = { bg = "$rc.fill.selected" },
					on_click = actions.retry_processing,
					Icon { name = "retry", size = 14, color = "$rc.text.primary" },
					ui.text { "Retry", text_size = 13, weight = 500, color = "$rc.text.primary" },
				},
				ui.button {
					id = "fix-provider",
					h = 28,
					px = 10,
					items = "center",
					rounded = 6,
					bg = "$rc.transparent",
					hover = { bg = "$rc.fill.hover" },
					on_click = actions.fix_providers,
					ui.text { "Use API key…", text_size = 13, weight = 500, color = "$rc.accent" },
				},
			},
		}
	)
end

return function(): Node
	local rows = {}
	for i = 1, STEPS do
		table.insert(rows, StepRow(i))
	end
	return ui.show(
		function()
			return processing.active(host.processing:get())
		end,
		ui.column {
			w_full = true,
			shrink = 0,
			px = 16,
			pt = 16,
			ui.column {
				id = "processing",
				w_full = true,
				p = 4,
				rounded = 8,
				bg = "$rc.fill.hover",
				border = 1,
				border_color = "$rc.stroke.hairline",
				role = "status",
				rows,
				Failure(),
			},
		}
	)
end
