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

return M
