--!strict
-- The UI's own state (what Rust doesn't need to know). Host state is in
-- host.luau.
export type NotesTab = "notes" | "transcript" | "summary"

local M = {}

M.notes_tab = ui.state("notes_tab", "transcript" :: NotesTab)
-- My notes (GLOSSARY): what the user types in the Notes tab.
M.my_notes = ui.state("my_notes", "")
-- The action panel (Ctrl K): the window it is open in ("" closed), its
-- search and its selected action ("": the first shown).
export type Where = "home" | "notes"
M.actions_open = ui.state("actions_open", "" :: Where | "")
M.actions_query = ui.state("actions_query", "")
M.actions_selected = ui.state("actions_selected", "")
-- Delete meeting asks first: the meeting, and the window asking ("" none).
export type ConfirmDelete = { id: string, title: string, where: Where | "" }
M.confirm_delete = ui.state("confirm_delete", { id = "", title = "", where = "" } :: ConfirmDelete)
-- The HUD pill ("Copied"), briefly; `seq` tells one from the next.
M.hud = ui.state("hud", { text = "", seq = 0 })
-- Whether the live transcript rests at its end (follows new rows).
M.transcript_at_end = ui.state("transcript_at_end", true)
-- Rows that arrived since the transcript was scrolled away from the end.
M.seen_count = ui.state("seen_count", 0)
-- The overlay: its hover toolbar is showing (after a 150 ms hover); no
-- speech for 8 s (the text dims); the last shift up of its lines, in
-- points, how many shifts there have been (wrap or new turn), and all
-- shifts added up (the slide eases that back to 0).
M.overlay_toolbar = ui.state("overlay_toolbar", false)
M.captions_idle = ui.state("captions_idle", false)
M.caption_shift = ui.state("caption_shift", { px = 0, count = 0, total = 0 })
-- The stored transcript's row last clicked (Play from here ↵); 0 none.
M.selected_row = ui.state("selected_row", 0)
-- The row whose speaker chip is being renamed inline (screens/5): its
-- index (0 none) and speaker id; what the field holds; the suggestion
-- highlighted (from 1).
M.renaming = ui.state("renaming", { index = 0, speaker = "" })
M.rename_text = ui.state("rename_text", "")
M.rename_pick = ui.state("rename_pick", 1)
-- Home: the selected meeting's id ("": the first shown) and the search text.
M.home_selected = ui.state("home_selected", "")
M.home_query = ui.state("home_query", "")

-- Settings: the page shown (screens/settings/pages.luau ids) and the
-- sidebar's search.
M.settings_page = ui.state("settings_page", "general")
M.settings_query = ui.state("settings_query", "")
-- The glossary term being typed.
M.glossary_draft = ui.state("glossary_draft", "")
-- The hotkey (Shortcuts) whose keys are being recorded; "" none. Shared:
-- the host lets the global hotkeys rest meanwhile, so the keys reach here.
M.hotkey_recording = ui.shared("hotkey_recording", "")

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
