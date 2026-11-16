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
	id = "overlay",
	title = "Recap captions",
	open = host.windows.overlay,
	width = 720,
	height = 120,
	frame_autosave = "overlay",
	Overlay(),
}

return nil
