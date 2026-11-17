--!strict
-- A text button, with the keys that do the same shown as keycaps after it
-- (Raycast's action bar: "Open Meeting ↵", "Actions Ctrl K"). The keys are
-- its description for assistive technology (lib/keys).
--
-- Looks: "bare" (an action bar's: no fill until hovered), "filled" (a
-- dialog's: the input fill, growing to share the row), "outlined" (a
-- settings row's: a field's hairline round it), "tinted" (a card's own
-- action: the selected fill, an icon before: Retry, Write summary, Test),
-- "link" (accent words: Use API key…, Name speakers).
local Icon = require("@components/icon")
local Keycap = require("@components/keycap")
local keys_lib = require("@lib/keys")

export type Props = {
	id: string?,
	-- Its words, or a function of them ("Save" / "Add Provider").
	label: string | () -> string,
	-- The words' id (tests read it).
	label_id: string?,
	on_click: () -> (),
	keys: { string }?,
	-- The keycaps' row id (tests read it).
	keys_id: string?,
	-- Whether the keycaps show (Stop's ↵ hides while paused).
	keys_shown: (() -> boolean)?,
	-- A token, or a function of one; primary by default.
	color: any?,
	weight: number?,
	look: ("bare" | "filled" | "outlined" | "tinted" | "link")?,
	-- An icon before the words (14 px).
	icon: string?,
	-- 24 tall, 4 aside: in a row of 24 px chips.
	compact: boolean?,
	-- Takes the keyboard as it shows: a dialog's least destructive button.
	autofocus: boolean?,
}

return function(props: Props): Node
	local look = props.look or "bare"
	local caps: Node? = nil
	if props.keys and #props.keys > 0 then
		local row: { [any]: any } = { id = props.keys_id, gap = 4, items = "center" }
		for _, key in props.keys do
			table.insert(row, Keycap(key))
		end
		caps = if props.keys_shown then ui.show(props.keys_shown, ui.row(row)) else ui.row(row)
	end
	return ui.button {
		id = props.id,
		label = props.label :: any,
		description = if props.keys and #props.keys > 0 then keys_lib.spoken(props.keys) else nil,
		h = if props.compact then 24 elseif look == "filled" then 32 else 28,
		px = if props.compact then 4 elseif look == "bare" then 6 else 10,
		gap = if props.icon then 6 else 8,
		shrink = 0,
		grow = if look == "filled" then 1 else nil,
		basis = if look == "filled" then 0 else nil,
		items = "center",
		justify = "center",
		rounded = 6,
		bg = if look == "filled" then "$rc.fill.input" elseif look == "tinted" then "$rc.fill.selected" else "$rc.transparent",
		border = if look == "outlined" then 1 else nil,
		border_color = if look == "outlined" then "$rc.stroke.field" else nil,
		hover = { bg = if look == "tinted" then "$rc.fill.selected" else "$rc.fill.hover" },
		-- On the next frame: a dialog opening focuses itself after its
		-- content mounts.
		on_mount = if props.autofocus
			then function(el)
				ui.after(1, function()
					el:focus()
				end)
			end
			else nil,
		on_click = props.on_click,
		if props.icon then Icon { name = props.icon, size = 14, color = props.color or "$rc.text.primary" } else nil,
		ui.text {
			props.label :: any,
			id = props.label_id,
			text_size = 13,
			weight = props.weight or 500,
			color = props.color or (if look == "link" then "$rc.accent" else "$rc.text.primary"),
		},
		caps,
	}
end
