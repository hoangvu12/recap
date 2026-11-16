--!strict
-- Reading a meeting's processing (host.processing): pure, so screens and
-- actions agree on it.
local M = {}

type Step = { id: string, status: string, took_ms: number, error: string, label: string }
type Processing = { id: string, steps: { Step } }

-- The card shows: steps to show, not all done.
function M.active(p: Processing): boolean
	for _, step in p.steps do
		if step.status ~= "done" then
			return true
		end
	end
	return false
end

-- The step that failed, if one did.
function M.failed(p: Processing): Step?
	for _, step in p.steps do
		if step.status == "error" then
			return step
		end
	end
	return nil
end

-- Still working: active and nothing failed.
function M.working(p: Processing): boolean
	return M.active(p) and M.failed(p) == nil
end

-- The transcript shown is the live one until Transcribing is done.
function M.transcribing(p: Processing): boolean
	for _, step in p.steps do
		if step.id == "transcribing" then
			return step.status ~= "done"
		end
	end
	return false
end

-- "11.5 s": how long a step took.
function M.took(ms: number): string
	return string.format("%.1f s", ms / 1000)
end

return M
