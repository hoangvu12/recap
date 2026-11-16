--!strict
-- Settings, Captions: the overlay's text (screens/8's Captions card, with
-- how many lines it shows).
local sections = require("@screens/settings/sections")

return function(): Node
	return ui.column { id = "page-captions", w_full = true, gap = 24, sections.Captions(true) }
end
