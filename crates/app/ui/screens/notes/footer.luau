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
local Keycap = require("@components/keycap")

local function bare_button(props: { id: string, label: string, color: any, on_click: () -> (), [number]: Child }): Node
	return ui.button {
		id = props.id,
		h = 28,
		px = 6,
		gap = 8,
		items = "center",
		rounded = 6,
		bg = "$rc.transparent",
		hover = { bg = "$rc.fill.hover" },
		on_click = props.on_click,
		ui.text { props.label, text_size = 13, color = props.color },
		{ table.unpack(props) },
	}
end

return function(): Node
	local live = function()
		local s = host.meeting:get().state
		return s == "recording" or s == "paused"
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
						bare_button {
							id = "resume",
							label = "Resume",
							color = "$rc.text.primary",
							on_click = actions.toggle_pause,
							Keycap("↵"),
						}
					),
					bare_button {
						id = "stop",
						label = "Stop",
						color = function()
							return if paused() then "$rc.text.secondary" else "$rc.text.primary"
						end,
						on_click = actions.stop,
						ui.show(function()
							return not paused()
						end, Keycap("↵")),
					},
					ui.div { w = 1, h = 16, mx = 4, bg = "$rc.stroke.hairline" },
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
						working = bare_button {
							id = "hide",
							label = "Hide",
							color = "$rc.text.primary",
							on_click = actions.hide_notes,
							Keycap("Esc"),
						},
						failed = bare_button {
							id = "retry-footer",
							label = "Retry",
							color = "$rc.text.primary",
							on_click = actions.retry_processing,
							Keycap("↵"),
						},
					}),
					ui.div { w = 1, h = 16, mx = 4, bg = "$rc.stroke.hairline" },
				}
			),
			-- A stored transcript: Play from here ↵ (the line last clicked).
			ui.show(
				function()
					return not live()
						and not processing.active(host.processing:get())
						and state.notes_tab:get() == "transcript"
						and state.selected_row:get() > 0
				end,
				ui.row {
					items = "center",
					gap = 4,
					bare_button {
						id = "play-from-here",
						label = "Play from here",
						color = "$rc.text.primary",
						on_click = function()
							actions.play_line(state.selected_row:peek())
						end,
						Keycap("↵"),
					},
					ui.div { w = 1, h = 16, mx = 4, bg = "$rc.stroke.hairline" },
				}
			),
			bare_button {
				id = "actions",
				label = "Actions",
				color = "$rc.text.secondary",
				on_click = actions.toggle_actions,
				ui.row { id = "actions-keys", gap = 4, Keycap("Ctrl"), Keycap("K") },
			},
		},
	}
end
