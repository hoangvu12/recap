--!strict
-- Recap's UI entry. The app's root window is a hidden host window (the app
-- lives in the tray); every visible window is a `ui.window` (windows.luau).
--
--   host.luau     what Rust publishes and what the UI asks of it
--   state.luau    the UI's own state;  actions.luau  what the user can do
--   theme/        design tokens (generated) and the two themes
--   lib/          plain logic: icons (generated), formatting, motion
--   components/   small reusable views (chip, keycap, icon, rows)
--   screens/      a window's content: notes/, overlay
--   windows.luau  every window, declared once
--   toasts.luau   system toasts the host asks for (the call toast)
local theme = require("@theme")
local host = require("@app/host")
-- Motion tokens and reduced motion (its `reduce_motion` state lives here).
require("@lib/motion")

theme.setup()
-- Settings' theme (follow system, dark, light), applied as it changes.
theme.follow(function()
	return host.settings:get().theme
end)
require("@app/windows")
require("@app/toasts")

return ui.div {
	id = "root",
	-- The applied theme, for tests.
	ui.text {
		function()
			return ui.theme.current:get()
		end,
		id = "theme",
		w = 0,
		h = 0,
		overflow = "clip",
		opacity = 0,
	},
}
