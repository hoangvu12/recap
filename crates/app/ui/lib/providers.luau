--!strict
-- Providers as the Settings page shows them: what a job's picker offers,
-- capability chips, the Add provider list's sections.
local host = require("@app/host")

local M = {}

-- Provider `id`, speech or AI.
function M.find(view: host.Providers, id: string?): host.ProviderRow?
	if id == nil then
		return nil
	end
	for _, list in { view.stt, view.ai } do
		for _, row in list do
			if row.id == id then
				return row
			end
		end
	end
	return nil
end

function M.job(view: host.Providers, id: string): host.JobChoice?
	for _, job in view.jobs do
		if job.id == id then
			return job
		end
	end
	return nil
end

-- Whether `row` can do `job`: its family, and every capability it needs.
function M.can_do(row: host.ProviderRow, job: host.JobChoice): boolean
	if row.family ~= job.family then
		return false
	end
	for _, need in job.needs do
		if not (row.capabilities :: any)[need] then
			return false
		end
	end
	return true
end

-- What `job`'s pickers offer: only providers that can do it.
function M.eligible(view: host.Providers, job: host.JobChoice): { host.ProviderRow }
	local out = {}
	for _, row in (if job.family == "stt" then view.stt else view.ai) do
		if M.can_do(row, job) then
			table.insert(out, row)
		end
	end
	return out
end

-- The chips beside a speech engine's name (screens/8b).
function M.abilities(row: host.ProviderRow): { string }
	local out = {}
	local c = row.capabilities
	if c.live then
		table.insert(out, "Live")
	end
	if c.diarize then
		table.insert(out, "Speakers")
	end
	if c.keyterms then
		table.insert(out, "Keyterms")
	end
	return out
end

local GROUPS: { { id: host.Group, label: string } } = {
	{ id = "api", label = "API" },
	{ id = "local", label = "On this PC" },
	{ id = "cli", label = "Command line" },
	{ id = "custom", label = "Custom" },
}

export type Section = { label: string, items: { host.Preset } }

-- The Add provider list (screens/8c): presets under their group, those
-- whose name holds `query`.
function M.sections(presets: { host.Preset }, query: string): { Section }
	local q = string.lower(query)
	local out = {}
	for _, group in GROUPS do
		local items = {}
		for _, preset in presets do
			if preset.group == group.id and (q == "" or string.find(string.lower(preset.name), q, 1, true)) then
				table.insert(items, preset)
			end
		end
		if #items > 0 then
			table.insert(out, { label = group.label, items = items })
		end
	end
	return out
end

-- The presets in the order the list shows them.
function M.shown(presets: { host.Preset }, query: string): { host.Preset }
	local out = {}
	for _, section in M.sections(presets, query) do
		for _, preset in section.items do
			table.insert(out, preset)
		end
	end
	return out
end

function M.preset(presets: { host.Preset }, id: string): host.Preset?
	for _, preset in presets do
		if preset.id == id then
			return preset
		end
	end
	return nil
end

-- Which form a Test belongs to: a saved provider, or a new one.
function M.target(id: string, preset: string): string
	return if id ~= "" then id else `new:{preset}`
end

-- "1.4 s"
function M.latency(ms: number): string
	return string.format("%.1f s", ms / 1000)
end

return M
