//! A meeting out of Recap: as Markdown (Copy as Markdown, Export .md) and in
//! Explorer (Show recording in folder). Plain data from the store, no zgx;
//! the clipboard and the save dialog are the UI's (`ui/actions.luau`).

use std::path::{Path, PathBuf};

use recap_engine::Source;
use recap_storage::{MeetingId, MeetingInfo, Segment, Speaker, Store};

use crate::home::speaker_label;

/// `ms` as the transcript shows times: `1:05`, `1:02:05`.
fn clock(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

fn title(info: &MeetingInfo) -> &str {
    let title = info.title.trim();
    if title.is_empty() { "Untitled meeting" } else { title }
}

/// Who said `segment`: its speaker's label, else its side.
fn said_by(segment: &Segment, speakers: &[Speaker]) -> String {
    match &segment.speaker {
        Some(id) => speakers
            .iter()
            .find(|s| s.id == *id)
            .map(speaker_label)
            .unwrap_or_else(|| speaker_label(&Speaker { id: id.clone(), name: None })),
        None => match segment.source {
            Source::Me => "Me".into(),
            Source::Them => "Them".into(),
        },
    }
}

/// Meeting `id` as one Markdown document: the title, its date (`date`, as
/// the UI shows it: local time is the UI's), duration and speakers, the
/// summary, my notes, then the transcript line by line.
pub fn meeting_markdown(store: &Store, id: &MeetingId, date: &str) -> Result<String, String> {
    let meeting = store.meeting(id).map_err(|e| e.to_string())?.ok_or_else(|| format!("no meeting {id}"))?;
    let segments = store.transcript(id).map_err(|e| e.to_string())?;
    let info = &meeting.info;

    let mut md = format!("# {}\n\n", title(info));
    let mut facts = Vec::new();
    if !date.trim().is_empty() {
        facts.push(format!("**Date:** {}", date.trim()));
    }
    facts.push(format!("**Duration:** {}", clock(info.duration_ms)));
    if !info.speakers.is_empty() {
        let names: Vec<String> = info.speakers.iter().map(speaker_label).collect();
        facts.push(format!("**Speakers:** {}", names.join(", ")));
    }
    // Two spaces: a Markdown line break, one fact per line.
    md.push_str(&facts.join("  \n"));
    md.push_str("\n\n");

    if let Some(summary) = meeting.summary.as_ref().filter(|s| !s.markdown.trim().is_empty()) {
        md.push_str(&format!("## Summary\n\n{}\n\n", summary.markdown.trim()));
    }
    if !meeting.my_notes.trim().is_empty() {
        md.push_str(&format!("## My notes\n\n{}\n\n", meeting.my_notes.trim()));
    }
    if !segments.is_empty() {
        md.push_str("## Transcript\n\n");
        for segment in segments.iter().filter(|s| !s.text.trim().is_empty()) {
            md.push_str(&format!(
                "**{}** ({}): {}\n\n",
                said_by(segment, &info.speakers),
                clock(segment.start_ms),
                segment.text.trim()
            ));
        }
    }
    Ok(format!("{}\n", md.trim_end()))
}

/// The name Export suggests: the title, without what Windows refuses in a
/// file name.
pub fn file_name(info: &MeetingInfo) -> String {
    let cleaned: String =
        title(info).chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { ' ' } else { c }).collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let name = words.join(" ");
    let name = name.trim_matches(|c: char| c == '.' || c.is_whitespace());
    format!("{}.md", if name.is_empty() { "Meeting" } else { name })
}

/// What Show recording in folder shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reveal {
    /// The recording, selected in its folder.
    Select(PathBuf),
    /// The meeting's folder (its recording is gone, or never was).
    Open(PathBuf),
}

pub fn reveal_target(store: &Store, id: &MeetingId) -> Reveal {
    if store.has_recording(id) { Reveal::Select(store.recording(id)) } else { Reveal::Open(store.folder(id)) }
}

/// Show `target` in Explorer.
pub fn reveal(target: &Reveal) -> std::io::Result<()> {
    let mut explorer = std::process::Command::new("explorer.exe");
    // Explorer reads `/select,"C:\a b\audio.ogg"`, not the argument quoted whole.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        match target {
            Reveal::Select(path) => explorer.raw_arg(format!("/select,\"{}\"", windows_path(path))),
            Reveal::Open(path) => explorer.raw_arg(format!("\"{}\"", windows_path(path))),
        };
    }
    #[cfg(not(windows))]
    match target {
        Reveal::Select(path) | Reveal::Open(path) => explorer.arg(path),
    };
    // Explorer's exit code says nothing useful (1 on success).
    explorer.spawn().map(drop)
}

fn windows_path(path: &Path) -> String {
    path.display().to_string().replace('/', "\\")
}
