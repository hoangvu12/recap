--!strict
-- Tabs (WAI-ARIA's tab list): a list that is one Tab stop, the arrows along
-- it moving between tabs (zgx's `focus_group`), Home and End to its ends.
-- A tab says whether it is `selected`; Enter, Space or a click shows it
-- (APG's manual activation, Base UI's default: a tab focused again as its
-- window comes back must not take over from a tab the app chose). Disabled
-- tabs stay in the list but do nothing.
--
-- The tab's look is the caller's: its props go on the tab's button.
local M = {}

-- `label` (its name), `vertical`, the tabs as children; layout props pass
-- through.
export type ListProps = { [any]: any }

function M.List(props: ListProps): Node
	local out: { [any]: any } = {}
	for k, v in props :: any do
		if k ~= "vertical" then
			out[k] = v
		end
	end
	out.role = "tablist"
	out.focus_group = if props.vertical then "vertical" else "horizontal"
	return (if props.vertical then ui.column else ui.row)(out)
end

-- `id`, `selected: () -> boolean`, `on_select`, `disabled: (() -> boolean)?`,
-- its face as children; style props pass through to its button.
export type TabProps = { [any]: any }

function M.Tab(props: TabProps): Node
	local out: { [any]: any } = {}
	for k, v in props :: any do
		if k ~= "on_select" then
			out[k] = v
		end
	end
	local select = props.on_select
	local disabled = props.disabled
	out.role = "tab"
	out.on_click = function()
		if not (disabled and disabled()) then
			select()
		end
	end
	return ui.button(out)
end

return M
