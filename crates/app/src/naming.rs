//! Step 2 of processing, Naming speakers (GLOSSARY: Speaker naming).
//!
//! Me is the name from Settings. The voices of Them (`S1..Sn`) still
//! unnamed go to the Summary and names job's AI with the refined transcript,
//! each turn labelled by its speaker id; it answers
//! `{S1: {name|null, evidence, confidence}, …}` ([`schema`]). A wrong name is
//! worse than "Speaker 3" (DESIGN §4.3, §8), so [`accept`] takes a name only
//! when the AI is sure, its evidence is a quote of the transcript that holds
//! the name, the name is not mine, and no other voice got it too. The answer
//! is kept as `names.json`; names given before (a rename, a rerun) stay.

use std::{collections::BTreeMap, sync::Arc};

use recap_engine::{BoxFuture, Llm, ProviderError};
use recap_storage::{Segment, Speaker};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    home::speaker_label,
    processing::{Job, Step, StepId, write_json},
};

/// The AI's answer, kept in the meeting's folder.
pub const NAMES: &str = "names.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// What the AI says about one voice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guess {
    pub name: Option<String>,
    /// A quote of the transcript that gives the name away.
    pub evidence: String,
    pub confidence: Confidence,
}

/// The answer's schema: one entry per voice in `ids`, each required.
pub fn schema(ids: &[&str]) -> Value {
    let guess = json!({
        "type": "object",
        "properties": {
            "name": { "type": ["string", "null"] },
            "evidence": { "type": "string" },
            "confidence": { "type": "string", "enum": ["high", "medium", "low"] }
        },
        "required": ["name", "evidence", "confidence"],
        "additionalProperties": false
    });
    let properties: serde_json::Map<String, Value> = ids.iter().map(|id| (id.to_string(), guess.clone())).collect();
    json!({
        "type": "object",
        "properties": properties,
        "required": ids,
        "additionalProperties": false
    })
}

/// `[m:ss] <label>: text`, one line per turn.
pub fn transcript_lines(segments: &[Segment], label: impl Fn(&Segment) -> String) -> String {
    let mut out = String::new();
    for s in segments {
        let secs = s.start_ms / 1000;
        out.push_str(&format!("[{}:{:02}] {}: {}\n", secs / 60, secs % 60, label(s), s.text));
    }
    out
}

/// The naming task for the voices `ids`: Me's turns carry `my_name` (or
/// "Me"), the voices' their ids.
pub fn prompt(segments: &[Segment], ids: &[&str], my_name: &str) -> String {
    let me = if my_name.trim().is_empty() { "Me" } else { my_name.trim() };
    let lines = transcript_lines(segments, |s| match s.speaker.as_deref() {
        Some("me") => me.to_owned(),
        Some(id) => id.to_owned(),
        None if s.source == recap_engine::Source::Me => me.to_owned(),
        None => "Them".to_owned(),
    });
    format!(
        "This is the transcript of a meeting. {me_line} The other voices are labelled {ids} by a \
         speech engine that tells voices apart but does not know who they are.\n\n\
         For each of {ids}, give the person's name only when the transcript says it plainly: they \
         introduce themselves (\"I'm Priya\"), someone addresses them by name right before or after \
         they speak (\"Priya, can you…\" then they answer; \"Thanks, Tom.\" right after Tom spoke), \
         or someone names them as the one who said or will do what they said. Never guess from \
         roles, topics or what is likely. When it is not plain, set name to null.\n\
         - name: the first name as said in the transcript, or null.\n\
         - evidence: the exact words spoken that show it, copied as they are (one sentence or \
           two), without the [m:ss] time and the label; empty when name is null.\n\
         - confidence: high only when the evidence leaves no doubt; medium or low otherwise.\n\
         {me_rule}\n\nTranscript:\n{lines}",
        me_line = if my_name.trim().is_empty() {
            "The user's own lines are labelled Me.".to_owned()
        } else {
            format!("The user is {me}; their own lines are labelled {me}.")
        },
        ids = ids.join(", "),
        me_rule = if my_name.trim().is_empty() {
            String::new()
        } else {
            format!("- None of {} is {me}: the user speaks on their own microphone.", ids.join(", "))
        },
    )
}

/// Lowercase words, punctuation as spaces, one space between: how quotes
/// and names are compared.
fn words(text: &str) -> String {
    let spaced: String = text.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A quote without the line marks the transcript was shown with
/// (`[1:41] S1: `): what was said in it.
fn spoken(quote: &str) -> String {
    let mut out = String::new();
    let mut rest = quote;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']').map(|c| open + c) else { break };
        let inside = &rest[open + 1..close];
        if inside.is_empty() || !inside.chars().all(|c| c.is_ascii_digit() || c == ':') {
            out.push_str(&rest[..=close]);
            rest = &rest[close + 1..];
            continue;
        }
        out.push_str(&rest[..open]);
        out.push(' ');
        rest = &rest[close + 1..];
        // The label after the time: a short run of words up to a colon.
        if let Some(colon) = rest.find(':')
            && colon <= 40
            && !rest[..colon].contains(['.', '?', '!', ','])
        {
            rest = &rest[colon + 1..];
        }
    }
    out.push_str(rest);
    out
}

/// Whether `inner` (words) is in `outer` (words) on word boundaries.
fn holds(outer: &str, inner: &str) -> bool {
    !inner.is_empty() && format!(" {outer} ").contains(&format!(" {inner} "))
}

/// The names to give, by voice id: only the guesses sure, said and unique
/// (module doc). `said` is the transcript's text.
pub fn accept(guesses: &BTreeMap<String, Guess>, said: &str, my_name: &str) -> BTreeMap<String, String> {
    let said = words(said);
    let mine = words(my_name);
    let mut taken: BTreeMap<String, String> = BTreeMap::new();
    for (id, guess) in guesses {
        let Some(name) = guess.name.as_deref().map(str::trim).filter(|n| !n.is_empty() && n.chars().count() <= 40)
        else {
            continue;
        };
        let key = words(name);
        let evidence = words(&spoken(&guess.evidence));
        let placeholder = matches!(key.as_str(), "me" | "them" | "you")
            || key.starts_with("speaker")
            || key.starts_with("unknown")
            || key.strip_prefix('s').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if guess.confidence != Confidence::High
            || placeholder
            || (!mine.is_empty() && key == mine)
            || !holds(&said, &evidence)
            || !holds(&evidence, &key)
        {
            continue;
        }
        taken.insert(id.clone(), name.to_owned());
    }
    // A name two voices got is nobody's.
    let mut count: BTreeMap<String, usize> = BTreeMap::new();
    for name in taken.values() {
        *count.entry(words(name)).or_default() += 1;
    }
    taken.retain(|_, name| count[&words(name)] == 1);
    taken
}

/// Step 2 with the Summary and names job's AI (an error when it has none).
pub struct NamingSpeakers {
    llm: Result<Arc<dyn Llm>, ProviderError>,
}

impl NamingSpeakers {
    pub fn new(llm: Result<Arc<dyn Llm>, ProviderError>) -> Self {
        Self { llm }
    }
}

impl Step for NamingSpeakers {
    fn id(&self) -> StepId {
        StepId::NamingSpeakers
    }

    fn run(&self, job: Job) -> BoxFuture<'static, Result<(), String>> {
        let llm = self.llm.clone();
        Box::pin(async move { name(llm, job).await })
    }
}

async fn name(llm: Result<Arc<dyn Llm>, ProviderError>, job: Job) -> Result<(), String> {
    let info =
        job.store.meeting(&job.id).map_err(|e| e.to_string())?.ok_or_else(|| format!("no meeting {}", job.id))?.info;
    let segments = job.store.transcript(&job.id).map_err(|e| e.to_string())?;
    let my_name = job.config.my_name.trim().to_owned();
    let voices: Vec<&Speaker> = info.speakers.iter().filter(|s| s.id != "me").collect();
    let unnamed = voices.iter().any(|s| s.name.is_none());
    let mut accepted = BTreeMap::new();
    if unnamed {
        let llm = llm.map_err(|e| e.message)?;
        let ids: Vec<&str> = voices.iter().map(|s| s.id.as_str()).collect();
        let answer = llm.complete_json(prompt(&segments, &ids, &my_name), schema(&ids)).await.map_err(|e| e.message)?;
        let path = job.folder.join(NAMES);
        write_json(&path, &answer).map_err(|e| format!("{}: {e}", path.display()))?;
        let guesses: BTreeMap<String, Guess> = serde_json::from_value(answer)
            .map_err(|e| format!("{} answered names that can't be read: {e}", llm.name()))?;
        let said: Vec<&str> = segments.iter().map(|s| s.text.as_str()).collect();
        accepted = accept(&guesses, &said.join(" "), &my_name);
        // A name another speaker has already is not given twice.
        let given: Vec<String> = info.speakers.iter().filter_map(|s| s.name.as_deref()).map(words).collect();
        accepted.retain(|_, name| !given.contains(&words(name)));
    }
    job.store
        .update(&job.id, |info| {
            for speaker in &mut info.speakers {
                if speaker.name.is_some() {
                    continue;
                }
                if speaker.id == "me" {
                    speaker.name = (!my_name.is_empty()).then(|| my_name.clone());
                } else if let Some(name) = accepted.get(&speaker.id) {
                    speaker.name = Some(name.clone());
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// What a speaker id is called in a transcript shown to the AI.
pub fn label_of(speakers: &[Speaker], segment: &Segment) -> String {
    let id = segment.speaker.as_deref().unwrap_or(if segment.source == recap_engine::Source::Me { "me" } else { "" });
    match speakers.iter().find(|s| s.id == id) {
        Some(speaker) => speaker_label(speaker),
        None if id.is_empty() => "Them".to_owned(),
        None => speaker_label(&Speaker { id: id.to_owned(), name: None }),
    }
}
