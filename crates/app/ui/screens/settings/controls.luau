--!strict
-- The controls on the right of a Settings row (screens/8): a switch, a
-- menu of choices, a − n px + stepper, a text field. Each shows a value it
-- reads and hands a change to `on_change`; ids are the row's `id`
-- (`set-<field>`) and its parts (`-state`, `-label`, `-<value>`, `-plus`…).
local Icon = require("@components/icon")
local motion = require("@lib/motion")

local M = {}

-- A 28×16 switch, accent when on (DESIGN §9.3 Switch).
function M.Switch(props: { id: string, label: string, on: () -> boolean, on_change: (boolean) -> () }): Node
	-- The thumb slides (transform only) with Raycast's slight overshoot,
	-- 366 ms; the track's colour eases.
	local on = ui.computed(props.on)
	-- Built again as reduced motion changes (motion.Live): the eases follow.
	return motion.Live(function(): Node
		return ui.button {
			id = props.id,
			label = props.label,
			role = "switch",
			selected = props.on,
			w = 28,
			h = 16,
			p = 0,
			shrink = 0,
			rounded = 999,
			bg = motion.tween(on, { from = "$rc.fill.selected", to = "$rc.accent", duration = motion.fast_in }),
			on_click = function()
				props.on_change(not props.on())
			end,
			ui.row {
				w_full = true,
				h_full = true,
				px = 2,
				items = "center",
				ui.div {
					w = 12,
					h = 12,
					rounded = 999,
					bg = "$rc.switch.thumb",
					x = motion.tween(on, { from = 0, to = 12, duration = motion.switch_ms, ease = motion.ease.switch }),
				},
				-- What it is, for tests and assistive technology; not drawn.
				ui.text {
					function()
						return if props.on() then "on" else "off"
					end,
					id = `{props.id}-state`,
					w = 0,
					h = 0,
					overflow = "clip",
					opacity = 0,
				},
			},
		}
	end)
end

export type Option = { value: string, label: string }

-- The chosen option's label and ⌃⌄; a menu of the options. `scope` keeps
-- its open state apart from the same menu on another page.
function M.Choice(props: {
	id: string,
	scope: string,
	label: string,
	value: () -> string,
	options: () -> { Option },
	on_change: (string) -> (),
}): Node
	local open = ui.state(`choice.{props.scope}.{props.id}`, false)
	local function label(): string
		local v = props.value()
		for _, o in props.options() do
			if o.value == v then
				return o.label
			end
		end
		return v
	end
	return ui.menu {
		open = open,
		p = 4,
		label = props.label,
		anchor = ui.button {
			id = props.id,
			h = 30,
			px = 6,
			gap = 8,
			shrink = 0,
			items = "center",
			rounded = 6,
			bg = "$rc.transparent",
			hover = { bg = "$rc.fill.hover" },
			on_click = ui.toggle(open),
			ui.text { label, id = `{props.id}-label`, literal = true, text_size = 13, color = "$rc.text.primary" },
			Icon { name = "chevrons-up-down", size = 14 },
		},
		build = function(): Node
			local items = {}
			local chosen = props.value()
			for _, o in props.options() do
				table.insert(
					items,
					ui.menu_item {
						id = `{props.id}-{o.value}`,
						label = o.label,
						on_click = function()
							props.on_change(o.value)
						end,
						ui.row {
							gap = 8,
							items = "center",
							ui.div {
								w = 16,
								h = 16,
								shrink = 0,
								if o.value == chosen then Icon { name = "check", size = 16 } else nil,
							},
							ui.text { o.label, literal = true, text_size = 13 },
						},
					}
				)
			end
			return ui.column { min_w = 200, items }
		end,
	}
end

local function StepButton(id: string, label: string, icon: string, on_click: () -> ()): Node
	return ui.button {
		id = id,
		label = label,
		w = 28,
		h = 28,
		p = 0,
		shrink = 0,
		items = "center",
		justify = "center",
		rounded = 6,
		bg = "$rc.transparent",
		border = 1,
		border_color = "$rc.stroke.field",
		hover = { bg = "$rc.fill.hover" },
		on_click = on_click,
		Icon { name = icon, size = 14, color = "$rc.text.primary" },
	}
end

-- [−] n unit [+], within min..max by step.
function M.Stepper(props: {
	id: string,
	label: string,
	value: () -> number,
	min: number,
	max: number,
	step: number,
	unit: string?,
	on_change: (number) -> (),
}): Node
	local function go(delta: number)
		local now = props.value()
		local next = math.clamp(now + delta, props.min, props.max)
		if next ~= now then
			props.on_change(next)
		end
	end
	return ui.row {
		id = props.id,
		gap = 8,
		shrink = 0,
		items = "center",
		StepButton(`{props.id}-minus`, `Smaller {props.label}`, "minus", function()
			go(-props.step)
		end),
		ui.row {
			gap = 6,
			items = "center",
			ui.text {
				function()
					return tostring(props.value())
				end,
				id = `{props.id}-value`,
				font = "$rc.font.mono",
				text_size = 13,
				color = "$rc.text.primary",
			},
			if props.unit
				then ui.text { props.unit, literal = true, font = "$rc.font.mono", text_size = 13, color = "$rc.text.primary" }
				else nil,
		},
		StepButton(`{props.id}-plus`, `Larger {props.label}`, "plus", function()
			go(props.step)
		end),
	}
end

-- A 32 px field (DESIGN §9.3 Field) that hands its text over on Enter or
-- when left.
function M.Field(props: {
	id: string,
	label: string,
	value: () -> string,
	placeholder: string?,
	w: number?,
	on_commit: (string) -> (),
}): Node
	return ui.input {
		id = props.id,
		label = props.label,
		value = props.value,
		placeholder = props.placeholder,
		placeholder_color = "$rc.text.tertiary",
		on_commit = props.on_commit,
		w = props.w or 160,
		h = 32,
		px = 10,
		shrink = 0,
		text_size = 13,
		rounded = 6,
		bg = "$rc.fill.input",
		border = 1,
		border_color = "$rc.stroke.field",
	}
end

return M
