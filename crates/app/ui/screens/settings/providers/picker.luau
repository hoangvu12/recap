--!strict
-- A job's main or backup picker (screens/8b "Use for"): the chosen
-- provider's logo and name, a menu of the providers that can do the job.
local host = require("@app/host")
local actions = require("@app/actions")
local providers = require("@lib/providers")
local Icon = require("@components/icon")
local Logo = require("@components/logo")

export type Props = { job: host.JobId, slot: "main" | "backup" }

local function Item(id: string, name: string, logo: string?, on_click: () -> ()): Node
	return ui.menu_item {
		id = id,
		label = name,
		on_click = on_click,
		ui.row {
			gap = 8,
			items = "center",
			if logo then Logo { name = logo } else ui.div { w = 16, h = 16 },
			ui.text { name, literal = true, text_size = 13 },
		},
	}
end

return function(props: Props): Node
	local job, slot = props.job, props.slot
	local base = `job-{job}-{slot}`
	local open = ui.state(`picker.{base}`, false)
	local chosen = ui.computed(function(): host.ProviderRow?
		local view = host.providers:get()
		local choice = providers.job(view, job)
		return if choice then providers.find(view, (choice :: any)[slot]) else nil
	end)
	-- The menu stands where its anchor is: a row that grows holds it.
	return ui.row { grow = 1, basis = 0, ui.menu {
		open = open,
		match_anchor_width = true,
		p = 4,
		label = `{job} {slot}`,
		anchor = ui.button {
			id = base,
			h = 30,
			w_full = true,
			px = 10,
			gap = 8,
			items = "center",
			rounded = 6,
			bg = "$rc.transparent",
			border = 1,
			border_color = "$rc.stroke.field",
			hover = { bg = "$rc.fill.hover" },
			on_click = ui.toggle(open),
			ui.row {
				w = 16,
				h = 16,
				shrink = 0,
				ui.each(
					ui.computed(function()
						local row = chosen:get()
						return if row then { row.logo } else {}
					end),
					function(_, _, logo)
						return Logo { name = logo:peek() }
					end,
					{
						key = function(logo: string): string
							return logo
						end,
					}
				),
			},
			ui.text {
				function()
					local row = chosen:get()
					return if row then row.name else "None"
				end,
				id = `{base}-name`,
				literal = true,
				grow = 1,
				basis = 0,
				truncate = true,
				text_size = 13,
				color = function()
					return if chosen:get() then "$rc.text.primary" else "$rc.text.tertiary"
				end,
			},
			Icon { name = "chevrons-up-down", size = 14 },
		},
		build = function(): Node
			local view = host.providers:peek()
			local choice = providers.job(view, job)
			local items = {}
			if choice then
				for _, row in providers.eligible(view, choice) do
					table.insert(
						items,
						Item(`pick-{job}-{slot}-{row.id}`, row.name, row.logo, function()
							actions.choose_provider(job, slot, row.id)
						end)
					)
				end
			end
			if slot == "backup" then
				table.insert(
					items,
					Item(`pick-{job}-{slot}-none`, "None", nil, function()
						actions.choose_provider(job, slot, nil)
					end)
				)
			end
			return ui.column { w_full = true, items }
		end,
	} }
end
