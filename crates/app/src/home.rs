//! Home's state (`ui/host.luau` holds the shapes): the meetings list and a
//! meeting's detail pane, the search. Plain data from the store, no zgx.

use recap_storage::{Meeting, MeetingInfo, Speaker, Status, Store};
use serde_json::{Value, json};

/// What a speaker is called: their name, else "Me" or "Speaker N".
pub fn speaker_label(speaker: &Speaker) -> String {
    if let Some(name) = speaker.name.as_deref().filter(|n| !n.trim().is_empty()) {
        return name.to_owned();
    }
    match speaker.id.strip_prefix('S') {
        Some(n) if n.parse::<u32>().is_ok() => format!("Speaker {n}"),
        _ if speaker.id == "me" => "Me".to_owned(),
        _ => speaker.id.clone(),
    }
}

/// A meeting's speakers as chips: the label and its place (from 1), which
/// picks the chip's hue.
pub fn speakers(info: &MeetingInfo) -> Value {
    info.speakers
        .iter()
        .enumerate()
        .map(|(i, s)| json!({ "label": speaker_label(s), "n": i + 1 }))
        .collect()
}

/// `meetings`: every meeting in `list` (newest first; Luau groups them by
/// day). `recover`: it never finished processing and its recording is still
/// in `store`, so processing it again can (Recover).
pub fn meetings(store: &Store, list: &[MeetingInfo]) -> Value {
    list.iter()
        .map(|m| {
            json!({
                "id": m.id.as_str(),
                "title": m.title,
                "started_ms": m.started_ms,
                "duration_ms": m.duration_ms,
                "status": m.status.as_str(),
                "recover": m.status != Status::Done && store.has_recording(&m.id),
            })
        })
        .collect()
}

/// `search_meetings(query)`: the ids of the meetings whose title, people,
/// transcript or summary hold every word of `query`, best match first.
pub fn search(store: &Store, query: &str) -> Value {
    match store.search(query) {
        Ok(ids) => ids.iter().map(|id| Value::from(id.as_str())).collect(),
        Err(error) => {
            eprintln!("recap: search {query:?}: {error}");
            json!([])
        }
    }
}

/// `meeting_detail(id)`: what the detail pane shows.
pub fn detail(meeting: &Meeting) -> Value {
    let info = &meeting.info;
    json!({
        "id": info.id.as_str(),
        "title": info.title,
        "overview": meeting.summary.as_ref().map_or("", |s| s.overview.as_str()),
        "started_ms": info.started_ms,
        "duration_ms": info.duration_ms,
        "status": info.status.as_str(),
        "speakers": speakers(info),
    })
}
