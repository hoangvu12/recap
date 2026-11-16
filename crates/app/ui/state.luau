--!strict
-- The UI's own state (what Rust doesn't need to know). Host state is in
-- host.luau.
export type NotesTab = "notes" | "transcript" | "summary"

local M = {}

M.notes_tab = ui.state("notes_tab", "transcript" :: NotesTab)
-- My notes (GLOSSARY): what the user types in the Notes tab.
M.my_notes = ui.state("my_notes", "")
-- The action panel (Ctrl K); its content comes with ticket #13.
M.actions_open = ui.state("actions_open", false)
-- Whether the live transcript rests at its end (follows new rows).
M.transcript_at_end = ui.state("transcript_at_end", true)
-- Rows that arrived since the transcript was scrolled away from the end.
M.seen_count = ui.state("seen_count", 0)
-- How many caption lines the overlay shows (1–3).
M.caption_lines = ui.state("caption_lines", 2)
-- Home: the selected meeting's id ("": the first shown) and the search text.
M.home_selected = ui.state("home_selected", "")
M.home_query = ui.state("home_query", "")

-- Settings: the page shown (screens/settings/pages.luau ids) and the
-- sidebar's search.
M.settings_page = ui.state("settings_page", "general")
M.settings_query = ui.state("settings_query", "")

-- The Add provider sheet over Settings: closed, the preset list
-- (screens/8c) or a provider's form (8d, 8e). `id` is the saved provider a
-- form edits ("" for a new one).
export type ProviderSheet = { screen: "closed" | "pick" | "form", preset: string, id: string }
M.provider_sheet = ui.state("provider_sheet", { screen = "closed", preset = "", id = "" } :: ProviderSheet)
M.provider_query = ui.state("provider_query", "")
-- The preset the list has selected ("": the first shown).
M.provider_selected = ui.state("provider_selected", "")

-- The open form's fields. The key is only what was typed.
M.form = {
	name = ui.state("form_name", ""),
	key = ui.state("form_key", ""),
	model = ui.state("form_model", ""),
	base_url = ui.state("form_base_url", ""),
	command = ui.state("form_command", ""),
	input = ui.state("form_input", "stdin"),
}

return M
