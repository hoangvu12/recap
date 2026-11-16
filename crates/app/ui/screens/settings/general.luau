--!strict
-- Settings, General (screens/8): You, Captions, Audio and Glossary as the
-- screen shows them, then Startup.
local sections = require("@screens/settings/sections")

return function(): Node
	return ui.column {
		id = "page-general",
		w_full = true,
		gap = 24,
		sections.You("general"),
		sections.Captions(false),
		sections.Audio("general"),
		sections.Glossary(),
		sections.Startup(),
	}
end
