--!strict
-- Recap's UI entry. The app's root window is a hidden host window (the app
-- lives in the tray); every visible window is a `ui.window` (windows.luau).
--
--   host.luau     what Rust publishes and what the UI asks of it
--   state.luau    the UI's own state;  actions.luau  what the user can do
--   theme/        design tokens (generated) and the two themes
--   lib/          plain logic: icons (generated), formatting
--   components/   small reusable views (chip, keycap, icon, rows)
--   screens/      a window's content: notes/, overlay
--   windows.luau  every window, declared once
--   toasts.luau   system toasts the host asks for (the call toast)
local theme = require("@theme")

theme.setup()
require("@app/windows")
require("@app/toasts")

return ui.div { id = "root" }
