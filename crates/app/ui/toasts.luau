--!strict
-- System toasts the host asks for (crates/app/src/calls.rs): "Teams call
-- detected / Start notes". Each new `seq` shows one, replacing the last;
-- clicking it asks the host, which decides what it means. A toast never acts
-- by itself.
local host = require("@app/host")

local M = {}

-- The last toast shown (state outlives a hot reload, so a reload doesn't
-- show it again).
local shown = ui.state("call_toast_shown", 0)

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
	}):after(nil, function(err)
		print(`recap: no toast: {err}`)
	end)
end)

return M
