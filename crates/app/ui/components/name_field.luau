--!strict
-- A name field (WAI-ARIA's editable combobox with a list, manual
-- selection): type a name, or pick one of the known people from the list
-- under the field; a name nobody has yet is the list's last row, `Add “…”`
-- (React Aria's and Ark's "create" item). What is typed is the name unless
-- one is picked (APG).
--
-- It looks like a field, ⌄ at its right showing everyone. The list sits
-- under the field, as wide (at least `min_w`), and takes no focus: keys
-- stay in the field. Typing opens it (its first row highlighted), ↓ opens
-- it or moves down, ↑ up, Enter takes the highlighted row, Escape closes it
-- (a second Escape goes on to the window: the dialog's Skip). A click
-- outside closes it.
--
-- Ids: the field `id`; the list `{list_id}`, its rows `{option_prefix}-{n}`
-- (the chosen one has `{option_prefix}-{n}-check`).
local speakers = require("@lib/speakers")
local motion = require("@lib/motion")
local Icon = require("@components/icon")
local IconButton = require("@components/icon_button")

export type Props = {
	id: string,
	label: string,
	text: State<string>,
	-- The known people to pick from.
	people: () -> { string },
	placeholder: string?,
	option_prefix: string?,
	list_id: string?,
	-- The field's box: its size, fill and border (a dialog's 34 px field,
	-- the inline rename's 26 px one).
	box: { [string]: any }?,
	text_size: number?,
	min_w: number?,
	-- Focused as it mounts.
	autofocus: boolean?,
	-- The list shows as long as the field does (the inline rename).
	always_open: boolean?,
	-- A row taken: the field gets its name by default.
	on_choose: ((name: string) -> ())?,
	-- Escape with nothing to close (the inline rename: leave it).
	on_escape: (() -> ())?,
	-- The list closed from outside (a click away), for `always_open`.
	on_dismiss: (() -> ())?,
}

local function same(a: string, b: string): boolean
	return string.lower(speakers.trim(a)) == string.lower(speakers.trim(b))
end

local function Row(props: Props, prefix: string, n: number, s: speakers.Suggestion, picked: () -> boolean, choose: (string) -> ()): Node
	local chosen = function(): boolean
		return not s.add and same(props.text:get(), s.name)
	end
	return ui.button {
		id = `{prefix}-{n}`,
		role = "option",
		label = s.label,
		selected = picked,
		focusable = false,
		w_full = true,
		h = 30,
		px = 8,
		gap = 8,
		items = "center",
		justify = "start",
		rounded = 6,
		bg = function()
			return if picked() then "$rc.fill.selected" else "$rc.transparent"
		end,
		hover = { bg = "$rc.fill.hover" },
		cursor = "pointer",
		on_click = function()
			choose(s.name)
		end,
		ui.div {
			w = 16,
			h = 16,
			shrink = 0,
			items = "center",
			justify = "center",
			if s.add
				then Icon { name = "plus", size = 14, color = "$rc.text.secondary" }
				else ui.show(chosen, Icon { name = "check", size = 14, color = "$rc.text.primary", id = `{prefix}-{n}-check` }),
		},
		ui.text {
			s.label,
			literal = true,
			grow = 1,
			basis = 0,
			truncate = true,
			text_size = 13,
			color = if s.add then "$rc.text.secondary" else "$rc.text.primary",
		},
	}
end

return function(props: Props): Node
	local prefix = props.option_prefix or `{props.id}-option`
	local open = ui.state(`name_field.{props.id}.open`, false)
	local pick = ui.state(`name_field.{props.id}.pick`, 1)
	local field: any = nil
	local focused = false
	-- The pointer is over the list: a press there takes focus from the
	-- field, but the list stays for its click.
	local over_list = false
	-- The name a row just put in the field: not typing, so the list stays
	-- closed for it.
	local chosen: string? = nil
	local options = ui.computed(function(): { speakers.Suggestion }
		return speakers.suggestions(props.people(), props.text:get(), nil)
	end)
	local numbered = ui.computed(function(): { { n: number, s: speakers.Suggestion } }
		local out = {}
		for n, s in options:get() do
			table.insert(out, { n = n, s = s })
		end
		return out
	end)

	local function close()
		if not props.always_open then
			open:set(false)
		end
	end
	local function choose(name: string)
		if props.on_choose then
			props.on_choose(name)
		else
			chosen = name
			props.text:set(name)
		end
		close()
		over_list = false
		-- The keyboard goes on in the field (a click in the list took it).
		if field and not props.on_choose then
			field:focus()
		end
	end

	-- Typing opens the list, its first row highlighted.
	local first = true
	ui.effect(function()
		local now = props.text:get()
		if first then
			first = false
			return
		end
		if chosen ~= nil then
			local was = chosen
			chosen = nil
			if now == was then
				return
			end
		end
		pick:set(1)
		if focused then
			open:set(true)
		end
	end)
	-- The inline rename's list shows while it does; closed from outside, the
	-- rename is left.
	if props.always_open then
		ui.effect(function()
			open:set(true)
		end)
		ui.effect(function()
			if not open:get() and props.on_dismiss then
				props.on_dismiss()
			end
		end)
	end

	local function on_key(e: any): boolean
		if e.ctrl or e.alt then
			return false
		end
		local list = options:peek()
		local shown = open:peek()
		local key = e.key
		if key == "ArrowDown" then
			if not shown then
				open:set(true)
				pick:set(1)
			else
				pick:set(math.min(pick:peek() + 1, math.max(#list, 1)))
			end
			return true
		elseif key == "ArrowUp" and shown then
			pick:set(math.max(pick:peek() - 1, 1))
			return true
		elseif key == "Enter" and shown then
			local row = list[pick:peek()]
			if row then
				choose(row.name)
			elseif props.on_escape then
				props.on_escape()
			else
				close()
			end
			return true
		elseif key == "Tab" then
			-- Focus moves on: the list goes (the key isn't taken).
			close()
			return false
		elseif key == "Escape" then
			if shown and not props.always_open then
				open:set(false)
				return true
			end
			if props.on_escape then
				props.on_escape()
				return true
			end
			return false
		end
		return false
	end

	local box: { [any]: any } = {
		relative = true,
		w_full = true,
		h = 34,
		pl = 12,
		pr = 4,
		gap = 4,
		items = "center",
		rounded = 6,
		bg = "$rc.fill.hover",
		border = 1,
		border_color = "$rc.stroke.field",
	}
	for k, v in props.box or {} do
		box[k] = v
	end

	table.insert(
		box,
		ui.input {
			id = props.id,
			label = props.label,
			value = props.text,
			placeholder = props.placeholder,
			placeholder_color = "$rc.text.tertiary",
			w = 0,
			grow = 1,
			h_full = true,
			p = 0,
			text_size = props.text_size or 13,
			bg = "$rc.transparent",
			border = 0,
			focus = { outline = 0 },
			active_descendant = function(): string?
				if not open:get() or #options:get() == 0 then
					return nil
				end
				return `{prefix}-{pick:get()}`
			end,
			on_mount = function(el)
				field = el
				if props.autofocus then
					el:focus()
				end
			end,
			on_focus = function()
				focused = true
			end,
			on_blur = function()
				focused = false
				if not over_list then
					close()
				end
			end,
			on_key = on_key,
		}
	)
	if not props.always_open then
		-- ⌄: everyone (as an empty field lists them); the keyboard stays in
		-- the field.
		table.insert(
			box,
			IconButton {
				id = `{props.id}-all`,
				icon = "chevrons-up-down",
				label = `Known people for {props.label}`,
				tip = "People",
				size = 24,
				icon_size = 14,
				rounded = 4,
				color = "$rc.text.tertiary",
				focusable = false,
				expanded = function()
					return open:get()
				end,
				on_click = function()
					local now = not open:peek()
					pick:set(1)
					open:set(now)
					if field then
						field:focus()
					end
				end,
			}
		)
	end
	-- The list, under the box (above it, with no room below) and as wide:
	-- anchored to an empty layer over the box that lets the pointer through.
	-- (`at_caret` keeps the keyboard in the field; with no editor in its
	-- anchor, zgx places it at the anchor's box.)
	table.insert(
		box,
		ui.popover {
			open = open,
			at_caret = true,
			label = props.label,
			match_anchor_width = true,
			placement = { side = "bottom", align = "start", offset = 4 },
			p = 0,
			rounded = 8,
			bg = "$rc.bg.raised",
			border = 1,
			border_color = "$rc.border.raised",
			shadow = { { x = 0, y = 8, blur = 10, spread = -6, color = "#0000001A" }, { x = 0, y = 20, blur = 25, spread = -5, color = "#00000033" } },
			anchor = ui.div { absolute = true, left = 0, right = 0, top = 0, bottom = 0, pointer_events = "none", aria_hidden = true },
			build = function(): Node
				-- Opens with a 150 ms fade and scale .98 → 1 (motion_spec §2.9).
				return motion.Enter("column", {
					id = props.list_id or `{props.id}-options`,
					role = "listbox",
					label = props.label,
					w_full = true,
					min_w = props.min_w,
					on_hover = function(inside: boolean)
						over_list = inside
					end,
					p = 4,
					gap = 2,
					ui.show(
						function()
							return #options:get() == 0
						end,
						ui.text { "No one yet", h = 30, px = 8, text_size = 13, color = "$rc.text.tertiary" }
					),
					ui.each(numbered, function(o: { n: number, s: speakers.Suggestion })
						local n = o.n
						return Row(props, prefix, n, o.s, function()
							return pick:get() == n
						end, choose)
					end, {
						-- By place and label: a row is rebuilt when either moves.
						key = function(o: { n: number, s: speakers.Suggestion }): string
							return `{o.n}|{o.s.label}`
						end,
					}),
				}, motion.POP)
			end,
		}
	)
	return ui.row(box)
end
