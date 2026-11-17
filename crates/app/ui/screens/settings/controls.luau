--!strict
-- The Settings-only controls on the right of a row (screens/8): a − n px +
-- stepper, a text field. (The switch and the menu of choices are the shared
-- components/switch and components/select.) Each shows a value it reads and
-- hands a change on; ids are the row's `id` (`set-<field>`) and its parts
-- (`-minus`, `-value`, `-plus`).
local IconButton = require("@components/icon_button")

local M = {}

local function StepButton(id: string, label: string, icon: string, on_click: () -> ()): Node
	return IconButton {
		id = id,
		icon = icon,
		label = label,
		on_click = on_click,
		outlined = true,
		color = "$rc.text.primary",
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
