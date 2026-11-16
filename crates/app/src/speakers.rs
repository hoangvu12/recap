//! Naming speakers by hand (screens/5 inline, screens/6 the Name speakers
//! dialog). Plain data from the store, no zgx.
//!
//! [`dialog`] is what the dialog shows: each speaker's chip, its name, two
//! quotes to tell the voices apart, the AI's hint (a guess `names.json` kept
//! but the naming step didn't take) and the known people. [`rename`] keeps
//! the names (the speakers and people tables; the refined transcript keeps
//! its speaker ids, so every view relabels from them). Me is locked: it is
//! the name from Settings.

use std::collections::BTreeMap;

use recap_storage::{MeetingId, Segment, Speaker, Store};
use serde_json::{Value, json};

use crate::{
    home::speaker_label,
    naming::{Guess, NAMES},
};

/// A quote is cut to about this many characters, on a word.
const QUOTE_CHARS: usize = 70;
/// A quote has at least this many words (to tell a voice by).
const QUOTE_WORDS: usize = 5;

/// `text` cut on a word to at most [`QUOTE_CHARS`], with "…" when cut.
fn shorten(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= QUOTE_CHARS {
        return text.to_owned();
    }
    let mut out = String::new();
    for word in text.split_whitespace() {
        let next = if out.is_empty() { word.to_owned() } else { format!("{out} {word}") };
        if next.chars().count() > QUOTE_CHARS {
            break;
        }
        out = next;
    }
    format!("{}…", out.trim_end_matches([',', ';', ':']))
}

/// Two quotes of speaker `id`: its first turn long enough to tell it by,
/// and its last.
pub fn quotes(segments: &[Segment], id: &str) -> Vec<String> {
    let theirs: Vec<&Segment> = segments.iter().filter(|s| s.speaker.as_deref() == Some(id)).collect();
    let long: Vec<&Segment> =
        theirs.iter().copied().filter(|s| s.text.split_whitespace().count() >= QUOTE_WORDS).collect();
    let picked: Vec<&Segment> = match long.as_slice() {
        [] => theirs.iter().copied().take(2).collect(),
        [one] => vec![*one],
        [first, .., last] => vec![*first, *last],
    };
    picked.iter().map(|s| shorten(&s.text)).collect()
}

/// The AI's guesses for meeting `id` (`names.json`), if it kept any.
fn guesses(store: &Store, id: &MeetingId) -> BTreeMap<String, Guess> {
    std::fs::read(store.folder(id).join(NAMES))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// `speakers_dialog` for meeting `id` (`ui/host.luau` has the shape).
pub fn dialog(store: &Store, id: &MeetingId) -> Result<Value, String> {
    let meeting = store.meeting(id).map_err(|e| e.to_string())?.ok_or_else(|| format!("no meeting {id}"))?;
    let segments = store.transcript(id).map_err(|e| e.to_string())?;
    let speakers = &meeting.info.speakers;
    let given: Vec<String> = speakers.iter().filter_map(|s| s.name.as_deref()).map(str::to_lowercase).collect();
    let guesses = guesses(store, id);
    let list: Vec<Value> = speakers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let suggest = match (&s.name, guesses.get(&s.id).and_then(|g| g.name.as_deref())) {
                (None, Some(name)) if !name.trim().is_empty() && !given.contains(&name.trim().to_lowercase()) => {
                    name.trim().to_owned()
                }
                _ => String::new(),
            };
            json!({
                "id": s.id,
                "label": speaker_label(&Speaker { id: s.id.clone(), name: None }),
                "n": i + 1,
                "name": s.name.as_deref().unwrap_or(""),
                "locked": s.id == "me",
                "quotes": quotes(&segments, &s.id),
                "suggest": suggest,
            })
        })
        .collect();
    let people = store.people().map_err(|e| e.to_string())?;
    Ok(json!({ "id": id.as_str(), "speakers": list, "people": people }))
}

/// The dialog with nothing in it.
pub fn no_dialog() -> Value {
    json!({ "id": "", "speakers": [], "people": [] })
}

/// Name the speakers of meeting `id` as `names` says (speaker id, name or
/// none to unname). Me is left as it is. The speakers whose name changed.
pub fn rename(store: &Store, id: &MeetingId, names: &[(String, Option<String>)]) -> Result<Vec<String>, String> {
    let info = store.meeting(id).map_err(|e| e.to_string())?.ok_or_else(|| format!("no meeting {id}"))?.info;
    let mut changed = Vec::new();
    for (speaker, name) in names {
        if speaker == "me" {
            continue;
        }
        let Some(before) = info.speakers.iter().find(|s| s.id == *speaker) else {
            return Err(format!("{id} has no speaker {speaker}"));
        };
        let name = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
        if before.name.as_deref() == name {
            continue;
        }
        store.rename_speaker(id, speaker, name).map_err(|e| e.to_string())?;
        changed.push(speaker.clone());
    }
    Ok(changed)
}
