//! The presenter: engine events in, the shared state the Luau UI reads out
//! (crates/app/ui/host.luau's types).

use std::{sync::Arc, time::Duration};

use recap::presenter::Presenter;
use recap_engine::{
    EngineEvent, Meeting, MeetingConfig, MeetingState, Script, ScriptedLiveStt, Segment, SegmentId,
    SilenceSource, Source,
};
use serde_json::json;

fn seg(id: u64, source: Source, start_ms: u64, text: &str, is_final: bool) -> EngineEvent {
    EngineEvent::Segment(Segment {
        id: SegmentId(id),
        source,
        start_ms,
        end_ms: start_ms + 1000,
        text: text.into(),
        is_final,
    })
}

#[test]
fn a_partial_is_an_open_row_and_a_caption() {
    let mut p = Presenter::new(1_000);
    let changed = p.apply(&seg(1, Source::Them, 2000, "Thanks, Tom", false));
    assert!(changed.transcript && changed.captions);
    assert_eq!(
        p.transcript(),
        json!({ "count": 1, "open": { "1": {
            "index": 1, "source": "them", "start_ms": 2000,
            "final": "", "partial": "Thanks, Tom", "open": true
        } } })
    );
    assert_eq!(p.captions(), json!([{ "source": "them", "final": "", "partial": "Thanks, Tom" }]));
}

#[test]
fn a_final_row_leaves_the_shared_state_and_is_served_on_demand() {
    let mut p = Presenter::new(0);
    p.apply(&seg(1, Source::Me, 0, "Okay, let's get", false));
    p.apply(&seg(1, Source::Me, 0, "Okay, let's get started.", true));
    assert_eq!(p.transcript(), json!({ "count": 1, "open": {} }));
    assert_eq!(p.row(1)["final"], "Okay, let's get started.");
    assert_eq!(p.row(1)["open"], false);
    assert_eq!(p.row(2), serde_json::Value::Null);
    assert_eq!(p.row(0), serde_json::Value::Null);
}

#[test]
fn the_meeting_follows_state_and_time() {
    let mut p = Presenter::new(1_794_823_200_000);
    assert_eq!(p.meeting()["state"], "idle");
    p.apply(&EngineEvent::State(MeetingState::Recording));
    p.apply(&EngineEvent::Elapsed(62_000));
    assert_eq!(
        p.meeting(),
        json!({ "state": "recording", "elapsed_ms": 62000, "title": "", "started_ms": 1_794_823_200_000u64, "id": "" })
    );
    let changed = p.apply(&EngineEvent::State(MeetingState::Paused));
    assert!(changed.meeting && !changed.transcript);
    assert_eq!(p.meeting()["state"], "paused");
}

#[test]
fn captions_are_the_last_three_lines_of_them() {
    let mut p = Presenter::new(0);
    for (i, (source, text)) in [
        (Source::Them, "one"),
        (Source::Me, "mine"),
        (Source::Them, "two"),
        (Source::Me, "mine again"),
        (Source::Them, "three"),
        (Source::Me, "and again"),
        (Source::Them, "four"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = i as u64 + 1;
        p.apply(&seg(id, source, id * 10_000, text, true));
    }
    let lines: Vec<_> = p.captions().as_array().unwrap().iter().map(|l| l["final"].clone()).collect();
    assert_eq!(lines, vec![json!("two"), json!("three"), json!("four")]);
}

#[test]
fn a_stopped_meeting_has_nothing_open() {
    let mut p = Presenter::new(0);
    p.apply(&seg(1, Source::Them, 0, "we launch on", false));
    p.apply(&EngineEvent::State(MeetingState::Stopped));
    assert_eq!(p.transcript()["open"], json!({}));
    assert_eq!(p.row(1)["final"], "we launch on");
    assert_eq!(p.meeting()["state"], "stopped");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_test_meeting_replays_into_the_ui_state() {
    let source = SilenceSource::new(Duration::from_secs(120));
    let stt = Arc::new(ScriptedLiveStt::new(Script::test_meeting()));
    let mut meeting =
        Meeting::start(MeetingConfig { caption_me: true, ..Default::default() }, Box::new(source), stt);
    let mut p = Presenter::new(0);
    while let Some(event) = meeting.next_event().await {
        p.apply(&event);
    }
    assert_eq!(p.transcript()["count"], 12);
    assert_eq!(p.transcript()["open"], json!({}));
    assert_eq!(p.row(1)["source"], "me");
    assert!(p.row(1)["final"].as_str().unwrap().starts_with("Okay, let's get started."));
    assert_eq!(p.row(2)["source"], "them");
    assert_eq!(p.meeting()["state"], "stopped");
}
