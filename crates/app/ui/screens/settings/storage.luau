--!strict
-- Settings, Storage: where meetings are kept (Change opens a folder
-- dialog; Home then lists the meetings there) and how long their audio is
-- kept (older audio is deleted, transcripts and summaries stay).
local host = require("@app/host")
local actions = require("@app/actions")
local Icon = require("@components/icon")
local parts = require("@screens/settings/parts")
local Select = require("@components/select")
local Button = require("@components/button")

local KEEP = {
	{ value = "0", label = "Forever" },
	{ value = "90", label = "90 days" },
	{ value = "30", label = "30 days" },
	{ value = "7", label = "7 days" },
	{ value = "1", label = "1 day" },
}

local function folder(): string
	local f = host.settings:get().storage_folder
	return if f == "" then "%LOCALAPPDATA%\\Recap" else f
end

return function(): Node
	return ui.column {
		id = "page-storage",
		w_full = true,
		gap = 6,
		parts.Section {
			title = "Storage",
			parts.Row {
				first = true,
				ui.text { "Meetings folder", shrink = 0, text_size = 13, color = "$rc.text.primary" },
				ui.row {
					grow = 1,
					basis = 0,
					gap = 6,
					items = "center",
					justify = "end",
					overflow = "clip",
					Icon { name = "folder", size = 14, color = "$rc.text.tertiary" },
					ui.text {
						folder,
						id = "storage-folder",
						literal = true,
						truncate = true,
						text_size = 13,
						color = "$rc.text.secondary",
					},
				},
				Button { id = "storage-change", label = "Change", look = "outlined", on_click = actions.choose_storage_folder },
			},
			parts.Row {
				ui.text { "Keep meeting audio", grow = 1, basis = 0, text_size = 13, color = "$rc.text.primary" },
				Select {
					id = "set-keep_audio_days",
					scope = "storage",
					label = "Keep meeting audio",
					value = function()
						return tostring(host.settings:get().keep_audio_days)
					end,
					options = function()
						return KEEP
					end,
					on_change = function(days: string)
						actions.set_settings({ keep_audio_days = tonumber(days) or 0 })
					end,
				},
			},
		},
		ui.show(
			function()
				local e = host.settings_error:get()
				return e.field == "storage" and e.message ~= ""
			end,
			ui.text {
				function()
					return host.settings_error:get().message
				end,
				id = "storage-error",
				text_size = 12,
				color = "$rc.red",
			}
		),
	}
end
