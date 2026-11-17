--!strict
-- A select (WAI-ARIA's select-only combobox, built on zgx's menu): a button
-- showing the chosen option's label and ⌃⌄; a menu of the options, the
-- chosen one checked and opened over the button. The button says
-- `expanded` while the menu is open. zgx's menu gives the keys: Up/Down,
-- Home/End, typing a letter, Enter, Escape back to the button.
--
-- Ids: the button is `id` (its label `{id}-label` or `label_id`), an option
-- `{item_prefix or id}-{option.key or option.value}`.
--
-- `trigger = "icon"` is a 24 px ⌃⌄ alone, with `label` as its tooltip: the
-- speakers dialog's list of known people beside a name field.
local Icon = require("@components/icon")
local IconButton = require("@components/icon_button")
local Logo = require("@components/logo")

export type Option = {
	value: string,
	label: string,
	-- A provider logo before the label.
	logo: string?,
	-- The id's end, when not the value ("none" for the empty value).
	key: string?,
}

export type Props = {
	id: string,
	-- Its accessible name (and the tooltip of an icon trigger).
	label: string,
	-- Keeps its open state apart from the same select elsewhere.
	scope: string,
	value: () -> string,
	options: () -> { Option },
	on_change: (string) -> (),
	item_prefix: string?,
	-- The shown label's id, when not `{id}-label`.
	label_id: string?,
	-- "value" (the chosen label and ⌃⌄), "field" (the same, a field's
	-- hairline round it, as wide as the row holding it) or "icon".
	trigger: ("value" | "field" | "icon")?,
	-- What the button says while nothing is chosen.
	placeholder: string?,
	-- An icon trigger's tooltip, when not the label.
	tip: string?,
	-- Menu width at least (points).
	min_w: number?,
}

local function find(options: { Option }, value: string): Option?
	for _, o in options do
		if o.value == value then
			return o
		end
	end
	return nil
end

local function Logos(chosen: () -> Option?): Node
	return ui.row {
		w = 16,
		h = 16,
		shrink = 0,
		ui.presence(function(): string?
			local o = chosen()
			return if o then o.logo else nil
		end, function(logo: string): Node
			return Logo { name = logo }
		end),
	}
end

return function(props: Props): Node
	local open = ui.state(`select.{props.scope}.{props.id}`, false)
	local prefix = props.item_prefix or props.id
	local trigger = props.trigger or "value"
	local has_logos = function(): boolean
		for _, o in props.options() do
			if o.logo then
				return true
			end
		end
		return false
	end
	local chosen = function(): Option?
		return find(props.options(), props.value())
	end
	local function item_id(o: Option): string
		return `{prefix}-{o.key or o.value}`
	end

	-- A menu's anchor is sized to its content: a field-wide trigger takes
	-- the width of the row round the menu (LEARNINGS: a 68 px picker).
	local field_width = ui.state(`select.{props.scope}.{props.id}.width`, 0)
	local anchor: Node
	if trigger == "icon" then
		anchor = IconButton {
			id = props.id,
			icon = "chevrons-up-down",
			label = props.label,
			tip = props.tip,
			size = 24,
			icon_size = 14,
			rounded = 4,
			color = "$rc.text.tertiary",
			expanded = function()
				return open:get()
			end,
			on_click = function()
				open:set(not open:peek())
			end,
		}
	else
		local field = trigger == "field"
		local width = field_width
		local button = ui.button {
			id = props.id,
			-- Its name and the chosen option, as a select says both.
			label = function(): string
				local o = chosen()
				return if o then `{props.label}, {o.label}` else props.label
			end,
			expanded = open,
			h = 30,
			w = if field
				then function(): number?
					local w = width:get()
					return if w > 0 then w else nil
				end
				else nil,
			px = if field then 10 else 6,
			gap = 8,
			shrink = 0,
			items = "center",
			rounded = 6,
			bg = "$rc.transparent",
			border = if field then 1 else nil,
			border_color = if field then "$rc.stroke.field" else nil,
			hover = { bg = "$rc.fill.hover" },
			cursor = "pointer",
			on_click = ui.toggle(open),
			if field then ui.show(has_logos, Logos(chosen)) else nil,
			ui.text {
				function(): string
					local o = chosen()
					if o then
						return o.label
					end
					return props.placeholder or props.value()
				end,
				id = props.label_id or `{props.id}-label`,
				literal = true,
				grow = if field then 1 else nil,
				basis = if field then 0 else nil,
				truncate = field,
				text_size = 13,
				color = function()
					local o = chosen()
					local empty = if o then o.value == "" else props.placeholder ~= nil
					return if empty then "$rc.text.tertiary" else "$rc.text.primary"
				end,
			},
			Icon { name = "chevrons-up-down", size = 14 },
		}
		anchor = button
	end

	local menu = ui.menu {
		open = open,
		p = 4,
		label = props.label,
		match_anchor_width = trigger == "field",
		open_over = if trigger == "icon"
			then nil
			else function(): string?
				local o = chosen()
				return if o then item_id(o) else nil
			end,
		anchor = anchor,
		build = function(): Node
			local items = {}
			local value = props.value()
			local logos = has_logos()
			for _, o in props.options() do
				local on = o.value == value
				table.insert(
					items,
					ui.menu_item {
						id = item_id(o),
						label = o.label,
						checked = on,
						radio = true,
						cursor = "pointer",
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
								if on then Icon { name = "check", size = 16, id = `{item_id(o)}-check` } else nil,
							},
							if logos then (if o.logo then Logo { name = o.logo } else ui.div { w = 16, h = 16 }) else nil,
							ui.text { o.label, literal = true, text_size = 13 },
						},
					}
				)
			end
			return ui.column { w_full = trigger == "field", min_w = props.min_w or 200, items }
		end,
	}
	if trigger ~= "field" then
		return menu
	end
	-- The menu stands where its anchor is: a row that grows holds it, and
	-- its width is the trigger's.
	return ui.row {
		grow = 1,
		basis = 0,
		on_resize = function(size)
			field_width:set(size.w)
		end,
		menu,
	}
end
