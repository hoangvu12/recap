--!strict
-- A job's main or backup picker (screens/8b "Use for"): a select showing
-- the chosen provider's logo and name, its menu the providers that can do
-- the job (and None, for a backup).
local host = require("@app/host")
local actions = require("@app/actions")
local providers = require("@lib/providers")
local Select = require("@components/select")

export type Props = { job: host.JobId, slot: "main" | "backup" }

return function(props: Props): Node
	local job, slot = props.job, props.slot
	local base = `job-{job}-{slot}`
	return Select {
		id = base,
		item_prefix = `pick-{job}-{slot}`,
		label_id = `{base}-name`,
		scope = "picker",
		label = `{job} {slot}`,
		trigger = "field",
		placeholder = "None",
		value = function(): string
			local view = host.providers:get()
			local choice = providers.job(view, job)
			return if choice then (choice :: any)[slot] or "" else ""
		end,
		options = function(): { Select.Option }
			local view = host.providers:get()
			local choice = providers.job(view, job)
			local out: { Select.Option } = {}
			if choice then
				for _, row in providers.eligible(view, choice) do
					table.insert(out, { value = row.id, label = row.name, logo = row.logo })
				end
			end
			if slot == "backup" then
				table.insert(out, { value = "", label = "None", key = "none" })
			end
			return out
		end,
		on_change = function(id: string)
			actions.choose_provider(job, slot, if id == "" then nil else id)
		end,
	}
end
