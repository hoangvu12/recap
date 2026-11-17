//! Step 3 of processing, Writing summary (GLOSSARY: Summary).
//!
//! The Summary and names job's AI gets the named transcript (each turn
//! labelled as the Transcript tab shows it), My notes and the meeting's date,
//! and answers [`SummaryData`] ([`schema`]: title, overview, topics, next
//! steps with owner and due, decisions). It is kept as the meeting's summary:
//! the data, its overview and its Markdown ([`markdown`], also summary.md).
//! The meeting takes the title unless the user gave it another one.
//!
//! Next steps tick on and off ([`set_done`]); a summary written again keeps
//! the ticks of the steps it writes again. [`view`] is the Summary tab's
//! state.

use std::sync::Arc;

use recap_engine::{BoxFuture, Llm, ProviderError};
use recap_storage::{MeetingId, Speaker, Store, Summary};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    home::speaker_label,
    naming::{label_of, transcript_lines},
    processing::{Job, Step, StepId},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub heading: String,
    pub bullets: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextStep {
    /// Who does it, as the transcript names them; none when nobody does.
    pub owner: Option<String>,
    pub text: String,
    /// When, short ("Thu, Nov 19", "Before Dec 1").
    pub due: Option<String>,
    /// Ticked in the Summary tab (not asked of the AI).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
}

/// What the summary step answers (the summary's `data`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryData {
    pub title: String,
    pub overview: String,
    pub topics: Vec<Topic>,
    pub next_steps: Vec<NextStep>,
    pub decisions: Vec<String>,
}

/// The answer's schema: every object closed, every field required (#10).
pub fn schema() -> Value {
    let strings = json!({ "type": "array", "items": { "type": "string" } });
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "overview": { "type": "string" },
            "topics": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "heading": { "type": "string" }, "bullets": strings },
                    "required": ["heading", "bullets"],
                    "additionalProperties": false
                }
            },
            "next_steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "owner": { "type": ["string", "null"] },
                        "text": { "type": "string" },
                        "due": { "type": ["string", "null"] }
                    },
                    "required": ["owner", "text", "due"],
                    "additionalProperties": false
                }
            },
            "decisions": strings
        },
        "required": ["title", "overview", "topics", "next_steps", "decisions"],
        "additionalProperties": false
    })
}

const DAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "Monday, November 16, 2026" for `ms` since the epoch (UTC).
pub fn date_words(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{}, {} {day}, {year}", DAYS[days.rem_euclid(7) as usize], MONTHS[(month - 1) as usize])
}

/// The summary task: the transcript as labelled, my notes, the date.
pub fn prompt(transcript: &str, my_notes: &str, started_ms: u64, me: &str) -> String {
    let notes = if my_notes.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\n\nMy notes, typed during the meeting: they show what I care about, so give those parts more \
             room (they are not a source of facts):\n{}\n",
            my_notes.trim()
        )
    };
    format!(
        "Write the notes of this meeting for {me}, who took part. It took place on {date}.\n\n\
         - title: what the meeting settled or was about, at most 7 words (\"Mobile app launch on Dec 1\").\n\
         - overview: 1 to 3 sentences: who met and what they decided.\n\
         - topics: the 1 to 5 subjects discussed, each a short heading and 1 to 4 short bullets with the \
           facts, numbers and open points.\n\
         - next_steps: every task someone agreed to do, one per task, in the order said. owner: the \
           person's name exactly as the transcript labels them (\"Speaker 2\" stays \"Speaker 2\"), or the \
           name they are called by when the label is a Speaker, or null when nobody took it. text: the task, \
           starting with a verb. due: when it is due, short, as a date like \"Thu, Nov 19\" when the day is \
           clear from the meeting's date, \"Before Dec 1\" for a deadline, or null.\n\
         - decisions: what was agreed, one sentence each.\n\
         Use only what the transcript says. Plain sentences, no Markdown.{notes}\n\nTranscript:\n{transcript}",
        date = date_words(started_ms),
    )
}

/// The summary as Markdown: the overview, a `###` section per topic, the
/// next steps as a task list, the decisions. Empty sections are left out.
pub fn markdown(data: &SummaryData) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !data.overview.trim().is_empty() {
        parts.push(format!("{}\n", data.overview.trim()));
    }
    let list = |items: &mut dyn Iterator<Item = String>| items.map(|i| format!("- {i}\n")).collect::<String>();
    for topic in &data.topics {
        parts.push(format!(
            "### {}\n{}",
            topic.heading.trim(),
            list(&mut topic.bullets.iter().map(|b| b.trim().to_owned()))
        ));
    }
    if !data.next_steps.is_empty() {
        let steps: String = data
            .next_steps
            .iter()
            .map(|s| {
                let owner = s.owner.as_deref().map(str::trim).filter(|o| !o.is_empty());
                let due = s.due.as_deref().map(str::trim).filter(|d| !d.is_empty());
                format!(
                    "- [{}] {}{}{}\n",
                    if s.done { "x" } else { " " },
                    owner.map(|o| format!("**{o}**: ")).unwrap_or_default(),
                    s.text.trim(),
                    due.map(|d| format!(" ({d})")).unwrap_or_default(),
                )
            })
            .collect();
        parts.push(format!("### Next steps\n{steps}"));
    }
    if !data.decisions.is_empty() {
        parts.push(format!("### Decisions\n{}", list(&mut data.decisions.iter().map(|d| d.trim().to_owned()))));
    }
    parts.join("\n")
}

fn stored(data: &SummaryData) -> Summary {
    Summary {
        overview: data.overview.trim().to_owned(),
        markdown: markdown(data),
        data: serde_json::to_value(data).unwrap_or(Value::Null),
    }
}

/// The Summary tab's `summary` for meeting `id` (`ui/host.luau` has the
/// shape). An owner who is one of `speakers` gets their place (from 1),
/// which picks the chip's hue; 0 otherwise.
pub fn view(id: &str, data: &SummaryData, speakers: &[Speaker]) -> Value {
    let place = |owner: &str| {
        speakers
            .iter()
            .position(|s| s.name.is_some() && speaker_label(s).eq_ignore_ascii_case(owner.trim()))
            .map_or(0, |i| i + 1)
    };
    json!({
        "id": id,
        "overview": data.overview,
        "topics": data.topics,
        "next_steps": data.next_steps.iter().enumerate().map(|(i, s)| json!({
            "index": i + 1,
            "owner": s.owner.as_deref().unwrap_or(""),
            "owner_n": s.owner.as_deref().map_or(0, place),
            "text": s.text,
            "due": s.due.as_deref().unwrap_or(""),
            "done": s.done,
        })).collect::<Vec<_>>(),
        "decisions": data.decisions,
    })
}

/// The Summary tab with nothing in it.
pub fn no_view() -> Value {
    view("", &SummaryData::default(), &[])
}

/// Meeting `id`'s summary data, if it has one that reads.
pub fn data_of(store: &Store, id: &MeetingId) -> Result<Option<SummaryData>, String> {
    let meeting = store.meeting(id).map_err(|e| e.to_string())?.ok_or_else(|| format!("no meeting {id}"))?;
    Ok(meeting.summary.and_then(|s| serde_json::from_value(s.data).ok()))
}

/// Tick next step `index` (from 0) of meeting `id` on or off, and keep it.
pub fn set_done(store: &Store, id: &MeetingId, index: usize, done: bool) -> Result<(), String> {
    let mut data = data_of(store, id)?.ok_or_else(|| format!("{id} has no summary"))?;
    let step = data.next_steps.get_mut(index).ok_or_else(|| format!("{id} has no next step {index}"))?;
    step.done = done;
    store.set_summary(id, &stored(&data)).map_err(|e| e.to_string())
}

/// Step 3 with the Summary and names job's AI (an error when it has none).
pub struct WritingSummary {
    llm: Result<Arc<dyn Llm>, ProviderError>,
}

impl WritingSummary {
    pub fn new(llm: Result<Arc<dyn Llm>, ProviderError>) -> Self {
        Self { llm }
    }
}

impl Step for WritingSummary {
    fn id(&self) -> StepId {
        StepId::WritingSummary
    }

    fn run(&self, job: Job) -> BoxFuture<'static, Result<(), String>> {
        let llm = self.llm.clone();
        Box::pin(async move { write(llm, job).await })
    }
}

async fn write(llm: Result<Arc<dyn Llm>, ProviderError>, job: Job) -> Result<(), String> {
    let meeting =
        job.store.meeting(&job.id).map_err(|e| e.to_string())?.ok_or_else(|| format!("no meeting {}", job.id))?;
    let segments = job.store.transcript(&job.id).map_err(|e| e.to_string())?;
    if segments.iter().all(|s| s.text.trim().is_empty()) {
        // Nothing was said: nothing to sum up.
        return Ok(());
    }
    let llm = llm.map_err(|e| e.message)?;
    let speakers = &meeting.info.speakers;
    let transcript = transcript_lines(&segments, |s| label_of(speakers, s));
    let me = speakers
        .iter()
        .find(|s| s.id == "me")
        .and_then(|s| s.name.clone())
        .or_else(|| (!job.config.my_name.trim().is_empty()).then(|| job.config.my_name.trim().to_owned()))
        .unwrap_or_else(|| "me".to_owned());
    let answer = llm
        .complete_json(prompt(&transcript, &meeting.my_notes, meeting.info.started_ms, &me), schema())
        .await
        .map_err(|e| e.message)?;
    let mut data: SummaryData = serde_json::from_value(answer)
        .map_err(|e| format!("{} answered a summary that can't be read: {e}", llm.name()))?;
    // Ticks stay on the steps written again.
    let before = meeting.summary.as_ref().and_then(|s| serde_json::from_value::<SummaryData>(s.data.clone()).ok());
    if let Some(before) = &before {
        for step in &mut data.next_steps {
            step.done = before.next_steps.iter().any(|b| b.done && b.text.trim() == step.text.trim());
        }
    }
    job.store.set_summary(&job.id, &stored(&data)).map_err(|e| e.to_string())?;
    // The generated title, unless the user gave the meeting another one.
    let generated_before = before.map(|b| b.title).unwrap_or_default();
    let title = data.title.trim().to_owned();
    job.store
        .update(&job.id, |info| {
            if !title.is_empty() && (info.title.is_empty() || info.title == generated_before) {
                info.title = title.clone();
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_as_words() {
        assert_eq!(date_words(0), "Thursday, January 1, 1970");
        // 2026-11-16 12:00 UTC.
        assert_eq!(date_words(1_794_830_400_000), "Monday, November 16, 2026");
        assert_eq!(date_words(951_782_400_000), "Tuesday, February 29, 2000");
    }
}
