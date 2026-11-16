--!strict
-- Settings, Glossary: names and product words, sent to the speech engine
-- as keyterms (at most 50, each up to 20 characters).
local sections = require("@screens/settings/sections")

return function(): Node
	return ui.column { id = "page-glossary", w_full = true, gap = 24, sections.Glossary() }
end
