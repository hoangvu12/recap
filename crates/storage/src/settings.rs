//! The user's settings (Settings window, screens/8): one typed struct, kept
//! in `settings.json` in Recap's home folder (`RECAP_DATA`, else
//! `%LOCALAPPDATA%\Recap`), not in the storage folder it names.
//!
//! Every field has a default, so a file from an older app (or a hand-edited
//! one) loads with what it has; [`Settings::load`] and [`Settings::save`]
//! bring values back into range ([`Settings::normalize`]).
//!
//! Readers: the overlay (caption size, lines, partial text, screen-share
//! hiding), capture (microphone, meeting audio), the speech engines
//! ([`Settings::keyterms`], [`Settings::live_options`]), the pipeline
//! (`my_name` is "Me"), the tray (hotkeys, call toast, autostart) and the
//! store (storage folder, audio retention).

use std::path::{Path, PathBuf};

use recap_engine::{AUTO_LANGUAGE, LiveOptions};
use serde::{Deserialize, Serialize};

use crate::{Result, files};

/// The most keyterms a speech engine takes (ElevenLabs: 50 × 20 chars).
pub const MAX_KEYTERMS: usize = 50;
/// The longest keyterm, in characters.
pub const KEYTERM_CHARS: usize = 20;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow Windows' light or dark.
    #[default]
    System,
    Dark,
    Light,
}

/// What "Them" is: all system audio but our own, or one app's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MeetingAudio {
    #[default]
    System,
    /// The app's executable (`ms-teams.exe`).
    App(String),
}

impl Serialize for MeetingAudio {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            MeetingAudio::System => "system",
            MeetingAudio::App(exe) => exe,
        })
    }
}

impl<'de> Deserialize<'de> for MeetingAudio {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.trim() {
            "" | "system" => MeetingAudio::System,
            exe => MeetingAudio::App(exe.to_owned()),
        })
    }
}

/// The global hotkeys (registered by the tray, #15), as `Ctrl+Alt+N`
/// ([`hotkey`]); "" is none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    /// Start new notes (or show the live meeting's).
    pub start_notes: String,
    /// Pause or resume the live meeting.
    pub pause: String,
    /// The overlay's click-through.
    pub click_through: String,
    /// Show or hide the caption overlay.
    pub captions: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            start_notes: "Ctrl+Alt+N".into(),
            pause: "Ctrl+Alt+P".into(),
            click_through: "Ctrl+Alt+C".into(),
            captions: "Ctrl+Alt+O".into(),
        }
    }
}

impl Hotkeys {
    /// Each hotkey's id and value, in Settings' order.
    pub fn all(&self) -> [(&'static str, &str); 4] {
        [
            ("start_notes", &self.start_notes),
            ("pause", &self.pause),
            ("click_through", &self.click_through),
            ("captions", &self.captions),
        ]
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut String> {
        match id {
            "start_notes" => Some(&mut self.start_notes),
            "pause" => Some(&mut self.pause),
            "click_through" => Some(&mut self.click_through),
            "captions" => Some(&mut self.captions),
            _ => None,
        }
    }
}

/// Everything Settings holds but the providers (`providers.json`) and their
/// keys (Credential Manager).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // General
    /// "Me" in transcripts and summaries; "" until set.
    pub my_name: String,
    pub theme: Theme,
    /// Open Recap when Windows starts (HKCU Run key, #15).
    pub autostart: bool,
    /// What meetings are spoken in, for live captions and the transcript:
    /// [`AUTO_LANGUAGE`] (detected, following a call that mixes two), or a
    /// code (`en`, `vi`). ADR 0022.
    pub language: String,

    // Captions (the overlay, #8)
    /// Open the caption bar as a meeting starts; off, Notes' Captions
    /// button (or its hotkey) opens it. ADR 0024.
    pub captions_on_start: bool,
    /// Caption my own speech too (dimmed); a second live socket for Me.
    pub caption_my_speech: bool,
    /// The overlay's text size, px ([`Settings::CAPTION_SIZES`]).
    pub caption_size: u32,
    /// How many lines the overlay shows, 1–3.
    pub caption_lines: u32,
    /// Show words still being recognised (grey).
    pub partial_text: bool,
    /// Hide the overlay from screen sharing (`WDA_EXCLUDEFROMCAPTURE`).
    pub hide_from_share: bool,

    // Audio (capture, #7)
    /// The microphone's device id; "" is the default communications device.
    pub microphone: String,
    pub meeting_audio: MeetingAudio,
    /// The "Teams is using your mic. Start notes?" toast (#15).
    pub suggest_on_call: bool,

    // Glossary
    /// Names and product words the speech engine should spell right.
    pub glossary: Vec<String>,

    // Shortcuts
    pub hotkeys: Hotkeys,

    // Storage
    /// Where meetings are kept; "" is Recap's home folder.
    pub storage_folder: String,
    /// Delete a finished meeting's audio after this many days; 0 keeps it.
    pub keep_audio_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            my_name: String::new(),
            theme: Theme::System,
            autostart: false,
            language: AUTO_LANGUAGE.to_owned(),
            captions_on_start: true,
            caption_my_speech: false,
            caption_size: 20,
            caption_lines: 2,
            partial_text: true,
            hide_from_share: true,
            microphone: String::new(),
            meeting_audio: MeetingAudio::System,
            suggest_on_call: true,
            glossary: Vec::new(),
            hotkeys: Hotkeys::default(),
            storage_folder: String::new(),
            keep_audio_days: 0,
        }
    }
}

/// Why a glossary term was not added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GlossaryError {
    #[error("empty")]
    Empty,
    #[error("longer than {KEYTERM_CHARS} characters")]
    TooLong,
    #[error("already there")]
    Duplicate,
    #[error("already {MAX_KEYTERMS} terms")]
    Full,
}

fn clean_term(term: &str) -> std::result::Result<String, GlossaryError> {
    let term = term.split_whitespace().collect::<Vec<_>>().join(" ");
    if term.is_empty() {
        Err(GlossaryError::Empty)
    } else if term.chars().count() > KEYTERM_CHARS {
        Err(GlossaryError::TooLong)
    } else {
        Ok(term)
    }
}

/// Add `term` to `terms` if it is valid, new (ignoring case) and fits.
fn push_term(terms: &mut Vec<String>, term: &str) -> std::result::Result<(), GlossaryError> {
    let term = clean_term(term)?;
    if terms.iter().any(|t| t.to_lowercase() == term.to_lowercase()) {
        return Err(GlossaryError::Duplicate);
    }
    if terms.len() >= MAX_KEYTERMS {
        return Err(GlossaryError::Full);
    }
    terms.push(term);
    Ok(())
}

impl Settings {
    /// The overlay's smallest and largest text size, px.
    pub const CAPTION_SIZES: (u32, u32) = (14, 40);

    /// The settings in `path`; the defaults when there is no file. A file
    /// that isn't JSON is an error (it is not overwritten with defaults).
    pub fn load(path: &Path) -> Result<Self> {
        Ok(files::read_json::<Settings>(path)?.unwrap_or_default().normalized())
    }

    /// Write them to `path` whole (a temporary file renamed over it).
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            files::ensure_dir(dir)?;
        }
        files::write_json(path, &self.clone().normalized())
    }

    /// Values brought into range: sizes clamped, the name and glossary
    /// trimmed, terms that can't be keyterms dropped, hotkeys written one
    /// way (one that doesn't parse, or repeats another, goes back to its
    /// default).
    pub fn normalized(mut self) -> Self {
        self.my_name = self.my_name.trim().to_owned();
        self.storage_folder = self.storage_folder.trim().to_owned();
        self.language = match self.language.trim().to_lowercase() {
            code if code.is_empty() => AUTO_LANGUAGE.to_owned(),
            code => code,
        };
        let (lo, hi) = Self::CAPTION_SIZES;
        self.caption_size = self.caption_size.clamp(lo, hi);
        self.caption_lines = self.caption_lines.clamp(1, 3);
        let mut glossary = Vec::new();
        for term in &self.glossary {
            let _ = push_term(&mut glossary, term);
        }
        self.glossary = glossary;
        let defaults = Hotkeys::default();
        let mut seen: Vec<String> = Vec::new();
        for (id, default) in defaults.all() {
            let slot = self.hotkeys.get_mut(id).expect("a known hotkey");
            let value =
                if slot.trim().is_empty() { Some(String::new()) } else { hotkey(slot).filter(|k| !seen.contains(k)) };
            *slot = value.unwrap_or_else(|| default.to_owned());
            if !slot.is_empty() {
                seen.push(slot.clone());
            }
        }
        self
    }

    /// Add `term` to the glossary.
    pub fn add_term(&mut self, term: &str) -> std::result::Result<(), GlossaryError> {
        push_term(&mut self.glossary, term)
    }

    /// Remove `term` (ignoring case) from the glossary.
    pub fn remove_term(&mut self, term: &str) {
        let term = term.trim().to_lowercase();
        self.glossary.retain(|t| t.to_lowercase() != term);
    }

    /// The glossary as keyterms: at most [`MAX_KEYTERMS`], each at most
    /// [`KEYTERM_CHARS`] characters.
    pub fn keyterms(&self) -> Vec<String> {
        self.keyterms_with(&[])
    }

    /// The glossary, then `extra` (attendees, known people) as long as
    /// there is room, without repeats.
    pub fn keyterms_with(&self, extra: &[String]) -> Vec<String> {
        let mut terms = Vec::new();
        for term in self.glossary.iter().chain(extra) {
            let _ = push_term(&mut terms, term);
        }
        terms
    }

    /// A live session's options: the keyterms, the language.
    pub fn live_options(&self) -> LiveOptions {
        LiveOptions { keyterms: self.keyterms(), language: Some(self.language.clone()) }
    }

    /// The folder meetings are kept in: the storage folder, else `home`.
    pub fn meetings_root(&self, home: &Path) -> PathBuf {
        if self.storage_folder.is_empty() { home.to_owned() } else { PathBuf::from(&self.storage_folder) }
    }
}

/// `keys` (`ctrl+alt+n`, `Shift+Ctrl+P`) written one way, modifiers first
/// in the order Ctrl, Alt, Shift, Win (`Ctrl+Alt+N`); `None` unless it is
/// one key with at least one modifier (F1–F24 may stand alone).
pub fn hotkey(keys: &str) -> Option<String> {
    let (mut ctrl, mut alt, mut shift, mut win) = (false, false, false, false);
    let mut key: Option<String> = None;
    for part in keys.split('+').map(str::trim) {
        match part.to_lowercase().as_str() {
            "" => return None,
            "ctrl" | "control" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            "win" | "super" | "meta" | "cmd" => win = true,
            other => {
                if key.is_some() {
                    return None;
                }
                key = Some(key_name(other)?);
            }
        }
    }
    let key = key?;
    let function_key =
        key.len() > 1 && key.starts_with('F') && key[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n));
    if !(ctrl || alt || shift || win) && !function_key {
        return None;
    }
    let mut out = Vec::new();
    for (on, name) in [(ctrl, "Ctrl"), (alt, "Alt"), (shift, "Shift"), (win, "Win")] {
        if on {
            out.push(name.to_owned());
        }
    }
    out.push(key);
    Some(out.join("+"))
}

/// A key's name as hotkeys write it: `N`, `5`, `F5`, `Space`…
fn key_name(key: &str) -> Option<String> {
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return c
            .is_ascii_alphanumeric()
            .then(|| c.to_ascii_uppercase().to_string())
            .or_else(|| ",.;/[]-=`'\\".contains(c).then(|| c.to_string()));
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
    {
        return Some(format!("F{n}"));
    }
    const NAMED: [&str; 13] = [
        "Space", "Enter", "Tab", "Home", "End", "PageUp", "PageDown", "Insert", "Delete", "Up", "Down", "Left", "Right",
    ];
    NAMED.iter().find(|n| n.eq_ignore_ascii_case(key)).map(|n| (*n).to_owned())
}
