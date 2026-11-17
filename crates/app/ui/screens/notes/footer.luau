--!strict
-- The Notes window's footer action bar (DESIGN §9.3): the rec dot and the
-- timer on the left; the primary action with ↵ and "Actions Ctrl K" on the
-- right.
local host = require("@app/host")
local actions = require("@app/actions")
local state = require("@app/state")
local format = require("@lib/format")
local processing = require("@lib/processing")
local RecDot = require("@components/rec_dot")
local Button = require("@components/button")
local ActionPanel = require("@components/action_panel")

-- The footer's labels are regular weight (Home's are 500).
local WEIGHT = 400

local function Divider(): Node
	return ui.div { w = 1, h = 16, mx = 4, bg = "$rc.stroke.hairline" }
end

return function(): Node
	local live = function()
		local s = host.meeting:get().state
		return s == "recording" or s == "paused"
	end
	-- ↵ plays the stored transcript from the line last clicked.
	local playing_from_here = function()
		return not live()
			and not processing.active(host.processing:get())
			and state.notes_tab:get() == "transcript"
			and state.selected_row:get() > 0
	end
	local paused = function()
		return host.meeting:get().state == "paused"
	end
	local notice = function(): string
		return host.meeting:get().notice or ""
	end
	return ui.column {
		id = "notes-footer",
		w_full = true,
		shrink = 0,
		-- One line the user should know (a provider failed, the backup took
		-- over); tertiary, as motion_spec §2.1's status row.
		ui.show(
			function()
				return notice() ~= ""
			end,
			ui.text {
				notice,
				id = "notice",
				w_full = true,
				px = 20,
				pb = 8,
				truncate = true,
				text_size = 11,
				color = "$rc.text.tertiary",
			}
		),
		ui.div { h = 1, w_full = true, bg = "$rc.stroke.hairline" },
		ui.row {
			h = 42,
			pl = 20,
			pr = 10,
			items = "center",
			gap = 8,
			RecDot(),
			ui.text {
				function()
					return format.clock(host.meeting:get().elapsed_ms)
				end,
				id = "timer",
				font = "$rc.font.mono",
				text_size = 13,
				color = "$rc.text.secondary",
				tabular = true,
			},
			ui.show(
				paused,
				ui.text { "Paused", id = "paused", text_size = 11, weight = 500, color = "$rc.text.tertiary" }
			),
			ui.div { grow = 1 },
			-- Recording: Stop ↵. Paused: Resume ↵ first, Stop beside it.
			ui.show(
				live,
				ui.row {
					items = "center",
					gap = 4,
					ui.show(
						paused,
						Button { id = "resume", label = "Resume", keys = { "↵" }, weight = WEIGHT, on_click = actions.toggle_pause }
					),
					Button {
						id = "stop",
						label = "Stop",
						weight = WEIGHT,
						color = function()
							return if paused() then "$rc.text.secondary" else "$rc.text.primary"
						end,
						keys = { "↵" },
						keys_shown = function()
							return not paused()
						end,
						on_click = actions.stop,
					},
					-- No divider after: Actions doesn't show while live.
				}
			),
			-- After Stop (screens/3a, 3b): Hide Esc while it processes, Retry ↵
			-- once a step failed.
			ui.show(
				function()
					return not live() and processing.active(host.processing:get())
				end,
				ui.row {
					items = "center",
					gap = 4,
					ui.switch(function()
						return if processing.failed(host.processing:get()) then "failed" else "working"
					end, {
						working = Button { id = "hide", label = "Hide", keys = { "Esc" }, weight = WEIGHT, on_click = actions.hide_notes },
						failed = Button {
							id = "retry-footer",
							label = "Retry",
							keys = { "↵" },
							weight = WEIGHT,
							on_click = actions.retry_processing,
						},
					}),
					Divider(),
				}
			),
			-- A stored transcript: Play from here ↵ (the line last clicked).
			ui.show(
				playing_from_here,
				ui.row {
					items = "center",
					gap = 4,
					Button {
						id = "play-from-here",
						label = "Play from here",
						keys = { "↵" },
						weight = WEIGHT,
						on_click = function()
							actions.play_line(state.selected_row:peek())
						end,
					},
					Divider(),
				}
			),
			-- Done (screens/9): Copy Markdown ↵ (↵ plays a line once one is
			-- clicked).
			ui.show(
				function()
					local m = host.meeting:get()
					return m.state == "stopped"
						and (m.id or "") ~= ""
						and not processing.active(host.processing:get())
						and not playing_from_here()
				end,
				ui.row {
					items = "center",
					gap = 4,
					Button {
						id = "copy-markdown",
						label = "Copy Markdown",
						keys = { "↵" },
						weight = WEIGHT,
						on_click = function()
							actions.run_action("notes", "copy_markdown")
						end,
					},
					Divider(),
				}
			),
			-- The panel acts on a stored meeting: while live it has nothing
			-- to offer, so Actions shows once the meeting is stopped (seen
			-- in the real app: a live Actions that did nothing). The panel
			-- hangs above it.
			ui.show(
				function()
					local m = host.meeting:get()
					return m.state == "stopped" and (m.id or "") ~= ""
				end,
				ActionPanel {
					where = "notes",
					anchor = Button {
						id = "actions",
						label = "Actions",
						color = "$rc.text.secondary",
						keys = { "Ctrl", "K" },
						keys_id = "actions-keys",
						weight = WEIGHT,
						on_click = function()
							actions.toggle_actions("notes")
						end,
					},
				}
			),
		},
	}
end
