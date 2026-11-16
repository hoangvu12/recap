--!strict
-- Every window of the app, declared once. Each is open while its
-- `window.<id>` shared state is true (Rust or the UI may set it). The Rust
-- host opens them with their native options (crates/app/src/windows.rs:
-- frameless, always on top, glass…) and the content built here.
--
-- To add a window: a `window.<id>` state in host.luau, a `ui.window` here
-- with a screen from screens/, and its options in windows.rs.
local host = require("@app/host")
local Home = require("@screens/home")
local Notes = require("@screens/notes")
local Overlay = require("@screens/overlay")
local Settings = require("@screens/settings")
local Speakers = require("@screens/speakers")
local theme = require("@theme")

ui.window {
	id = "home",
	title = "Recap",
	open = host.windows.home,
	width = 860,
	height = 600,
	frame_autosave = "home",
	Home(),
}

ui.window {
	id = "notes",
	title = "Recap",
	open = host.windows.notes,
	width = 420,
	height = 900,
	frame_autosave = "notes",
	Notes(),
}

ui.window {
	id = "settings",
	title = "Settings",
	open = host.windows.settings,
	width = 800,
	height = 732,
	frame_autosave = "settings",
	Settings(),
}

ui.window {
	id = "speakers",
	title = "Name speakers",
	open = host.windows.speakers,
	width = 480,
	height = 620,
	frame_autosave = "speakers",
	Speakers(),
}

ui.window {
	id = "overlay",
	title = "Recap captions",
	open = host.windows.overlay,
	width = 720,
	height = 120,
	frame_autosave = "overlay",
	-- Dark always, whatever the app's theme (DESIGN §9.3 Caption bar).
	ui.column {
		id = "overlay-scope",
		theme = theme.OVERLAY,
		w_full = true,
		h_full = true,
		ui.text { theme.OVERLAY, id = "overlay-theme", w = 0, h = 0, overflow = "clip", opacity = 0 },
		Overlay(),
	},
}

return nil
