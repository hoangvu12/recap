--!strict
-- A checkbox: a 16 px box (accent with a check when on) and what it is
-- about beside it; the whole row toggles it. It says `checked` (UI
-- Automation's Toggle). Ids: `id`, `{id}-box`, `{id}-checked` while on.
local Icon = require("@components/icon")

export type Props = {
	id: string,
	checked: () -> boolean,
	on_change: (boolean) -> (),
	-- Its name, when not its text.
	label: string?,
	[number]: Child,
}

return function(props: Props): Node
	local done = props.checked
	return ui.button {
		id = props.id,
		role = "checkbox",
		label = props.label,
		checked = done,
		w_full = true,
		py = 6,
		px = 0,
		gap = 10,
		items = "start",
		justify = "start",
		rounded = 0,
		bg = "$rc.transparent",
		hover = { bg = "$rc.transparent" },
		cursor = "pointer",
		on_click = function()
			props.on_change(not done())
		end,
		ui.div {
			id = `{props.id}-box`,
			w = 16,
			h = 16,
			mt = 2,
			shrink = 0,
			rounded = 4,
			items = "center",
			justify = "center",
			border = function()
				return if done() then 0 else 1
			end,
			border_color = "$rc.stroke.field",
			bg = function()
				return if done() then "$rc.accent" else "$rc.transparent"
			end,
			ui.show(done, Icon { name = "check", size = 12, color = "$rc.on.accent", id = `{props.id}-checked` }),
		},
		{ table.unpack(props) },
	}
end
