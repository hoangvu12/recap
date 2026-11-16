--!strict
-- Settings, Audio: the microphone (Me), the meeting audio (Them) and the
-- call toast. The microphones are listed again as the page opens.
local host = require("@app/host")
local sections = require("@screens/settings/sections")

return function(): Node
	return ui.column {
		id = "page-audio",
		w_full = true,
		gap = 24,
		on_mount = function()
			host.invoke("refresh_microphones")
		end,
		sections.Audio("audio"),
	}
end
