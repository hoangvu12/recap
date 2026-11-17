--!strict
-- System toasts the host asks for:
-- - the call toast (crates/app/src/calls.rs): "Teams call detected / Start
--   notes". Clicking it asks the host, which decides what it means. It
--   never acts by itself.
-- - "Summary ready" (host.rs, story 31): a meeting's summary is written
--   while its Notes window isn't showing it. Clicking it opens the meeting.
-- Each new `seq` shows one, replacing the last of its kind.
local host = require("@app/host")

local M = {}

-- The last toasts shown (state outlives a hot reload, so a reload doesn't
-- show them again).
local shown = ui.state("call_toast_shown", 0)
local summary_shown = ui.state("summary_toast_shown", 0)

local function failed(err)
	print(`recap: no toast: {err}`)
end

ui.effect(function()
	local toast = host.call_toast:get()
	if toast.seq == 0 or toast.seq == shown:peek() then
		return
	end
	shown:set(toast.seq)
	ui.notify({
		title = toast.title,
		body = toast.body,
		tag = "call",
		on_click = function()
			host.invoke("call_toast_clicked", toast.seq)
		end,
	}):after(nil, failed)
end)

ui.effect(function()
	local toast = host.summary_toast:get()
	if toast.seq == 0 or toast.seq == summary_shown:peek() then
		return
	end
	summary_shown:set(toast.seq)
	ui.notify({
		title = "Summary ready",
		body = toast.title,
		tag = "summary",
		on_click = function()
			host.invoke("open_meeting", toast.id)
		end,
	}):after(nil, failed)
end)

return M
