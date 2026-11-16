//! `recap --seed`: the synthetic test meeting (testdata/meeting_tts_script.txt:
//! Tom, Priya and David agree on a December 1 launch) stored as if it had
//! been recorded and processed, so Home has a meeting to show.

use recap_engine::{Script, ScriptedLiveStt};
use recap_storage::{MeetingInfo, Result, Segment, Speaker, Status, Store, Summary};
use serde_json::json;

pub const TITLE: &str = "Mobile app launch on Dec 1";

const OVERVIEW: &str = "Tom, Priya and David agreed to launch the mobile app on December 1, one week later \
than planned, so the backend can add caching. An India-only soft launch is the fallback.";

const MARKDOWN: &str = "\
Tom, Priya and David agreed to launch the mobile app on December 1, one week later than planned, so the backend can add caching. An India-only soft launch is the fallback.

### Status
- Design: onboarding screens are done; the new checkout flow is about 90% finished.
- Backend: the payments API is ready, but load testing showed slow responses above 2,000 users.

### Launch date
- One more week for caching moves the launch to December 1.
- If load testing still fails, launch softly in India only.

### Next steps
- [ ] **David**: Add caching to the payments API
- [ ] **David**: Send the load test report to the whole team (Thursday)
- [ ] **Priya**: Fix the dark mode colors on the payment page (Friday)
- [ ] **Priya**: Book a review of the new checkout flow with the support team (next Monday)
- [ ] **Tom**: Write the release notes and share a draft (Wednesday)

### Decisions
- Launch on December 1, with caching in place.
- The India-only soft launch is the backup plan.
";

/// Who says each line: the script's TTS voice, its speaker id and name
/// (Mark is Tom, on the mic).
fn speaker(voice: &str) -> (&'static str, &'static str) {
    match voice {
        "Mark" => ("me", "Tom"),
        "Zira" => ("S1", "Priya"),
        _ => ("S2", "David"),
    }
}

/// Store the test meeting, ended a few minutes before `now_ms`; once only
/// (seeding again returns the one there).
pub fn test_meeting(store: &Store, now_ms: u64) -> Result<MeetingInfo> {
    if let Some(info) = store.list()?.into_iter().find(|m| m.title == TITLE) {
        return Ok(info);
    }
    let script = Script::test_meeting();
    let times = ScriptedLiveStt::new(script.clone()).line_times();
    let duration_ms = times.last().map_or(0, |t| t.1);

    let info = store.create(now_ms.saturating_sub(duration_ms + 5 * 60_000))?;
    let id = &info.id;
    let segment = |i: usize, speaker: Option<&str>| {
        let line = &script.lines[i];
        Segment {
            source: line.source,
            speaker: speaker.map(Into::into),
            start_ms: times[i].0,
            end_ms: times[i].1,
            text: line.text.clone(),
        }
    };
    let live: Vec<Segment> = (0..script.lines.len()).map(|i| segment(i, None)).collect();
    store.append_live(id, &live)?;
    let refined: Vec<Segment> =
        (0..script.lines.len()).map(|i| segment(i, Some(speaker(&script.lines[i].voice).0))).collect();
    store.set_refined(id, &refined)?;
    store.set_summary(
        id,
        &Summary {
            overview: OVERVIEW.into(),
            markdown: MARKDOWN.into(),
            data: json!({
                "title": TITLE,
                "overview": OVERVIEW,
                "topics": [
                    { "heading": "Status", "bullets": [
                        "Design: onboarding screens are done; the new checkout flow is about 90% finished.",
                        "Backend: the payments API is ready, but load testing showed slow responses above 2,000 users."
                    ] },
                    { "heading": "Launch date", "bullets": [
                        "One more week for caching moves the launch to December 1.",
                        "If load testing still fails, launch softly in India only."
                    ] }
                ],
                "next_steps": [
                    { "owner": "David", "text": "Add caching to the payments API", "due": null },
                    { "owner": "David", "text": "Send the load test report to the whole team", "due": "Thursday" },
                    { "owner": "Priya", "text": "Fix the dark mode colors on the payment page", "due": "Friday" },
                    { "owner": "Priya", "text": "Book a review of the new checkout flow with the support team", "due": "next Monday" },
                    { "owner": "Tom", "text": "Write the release notes and share a draft", "due": "Wednesday" }
                ],
                "decisions": [
                    "Launch on December 1, with caching in place.",
                    "The India-only soft launch is the backup plan."
                ]
            }),
        },
    )?;
    store.update(id, |m| {
        m.title = TITLE.into();
        m.duration_ms = duration_ms;
        m.status = Status::Done;
        m.speakers = ["Mark", "Zira", "David"]
            .iter()
            .map(|voice| {
                let (id, name) = speaker(voice);
                Speaker { id: id.into(), name: Some(name.into()) }
            })
            .collect();
    })
}
