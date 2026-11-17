//! Home's state and a stored meeting in the Notes window, from a store
//! seeded with the test meeting.

use recap::{home, presenter::Presenter, seed};
use recap_storage::{Status, Store, TranscriptKind};

const NOW: u64 = 1_794_830_400_000;

/// How long the test meeting's replay runs.
fn replay_ms() -> u64 {
    let stt = recap_engine::ScriptedLiveStt::new(recap_engine::Script::test_meeting());
    stt.line_times().last().unwrap().1
}

fn seeded() -> (tempfile::TempDir, Store, recap_storage::MeetingId) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = seed::test_meeting(&store, NOW).unwrap().id;
    (dir, store, id)
}

#[test]
fn the_seed_is_the_test_meeting_named_and_summarised() {
    let (_dir, store, id) = seeded();
    let meeting = store.meeting(&id).unwrap().unwrap();
    assert_eq!(meeting.info.title, "Mobile app launch on Dec 1");
    assert_eq!(meeting.info.status, Status::Done);
    let names: Vec<_> = meeting.info.speakers.iter().map(|s| s.name.clone().unwrap()).collect();
    assert_eq!(names, ["Tom", "Priya", "David"]);
    // The replay's own length (about two minutes; screens/7 shows 1:58).
    assert_eq!(meeting.info.duration_ms, replay_ms());
    assert!(meeting.info.started_ms + meeting.info.duration_ms <= NOW);

    let summary = meeting.summary.unwrap();
    assert!(summary.overview.contains("December 1"));
    assert!(summary.markdown.contains("India"));
    assert_eq!(summary.data["next_steps"].as_array().unwrap().len(), 5);

    let refined = store.segments(&id, TranscriptKind::Refined).unwrap();
    assert_eq!(refined.len(), 12);
    assert_eq!(refined[0].speaker.as_deref(), Some("me"));
    assert_eq!(refined[1].speaker.as_deref(), Some("S1"));
    assert_eq!(refined[2].speaker.as_deref(), Some("S2"));
    assert_eq!(store.segments(&id, TranscriptKind::Live).unwrap().len(), 12);
}

#[test]
fn seeding_twice_keeps_one_test_meeting() {
    let (_dir, store, id) = seeded();
    assert_eq!(seed::test_meeting(&store, NOW + 60_000).unwrap().id, id);
    assert_eq!(store.list().unwrap().len(), 1);
}

#[test]
fn home_lists_each_meeting_with_its_time_and_state() {
    let (_dir, store, id) = seeded();
    let unfinished = store.create(NOW - 86_400_000).unwrap();
    let list = home::meetings(&store, &store.list().unwrap());
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["id"], id.as_str());
    assert_eq!(list[0]["title"], "Mobile app launch on Dec 1");
    assert_eq!(list[0]["duration_ms"], replay_ms());
    assert_eq!(list[0]["status"], "done");
    assert_eq!(list[1]["id"], unfinished.id.as_str());
    assert_eq!(list[1]["status"], "recording");
    assert_eq!(list[1]["started_ms"], NOW - 86_400_000);
}

#[test]
fn the_detail_has_the_overview_and_the_speakers_in_order() {
    let (_dir, store, id) = seeded();
    let detail = home::detail(&store.meeting(&id).unwrap().unwrap());
    assert!(detail["overview"].as_str().unwrap().starts_with("Tom, Priya and David agreed"));
    assert_eq!(
        detail["speakers"],
        serde_json::json!([{ "label": "Tom", "n": 1 }, { "label": "Priya", "n": 2 }, { "label": "David", "n": 3 }])
    );
}

#[test]
fn speakers_without_a_name_are_speaker_n() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = store.create(NOW).unwrap();
    store
        .update(&info.id, |m| {
            m.speakers = vec![
                recap_storage::Speaker { id: "me".into(), name: None },
                recap_storage::Speaker { id: "S2".into(), name: None },
            ]
        })
        .unwrap();
    let detail = home::detail(&store.meeting(&info.id).unwrap().unwrap());
    assert_eq!(detail["speakers"][0]["label"], "Me");
    assert_eq!(detail["speakers"][1]["label"], "Speaker 2");
    assert_eq!(detail["overview"], "");
}

#[test]
fn a_stored_meeting_reads_in_the_notes_window() {
    let (_dir, store, id) = seeded();
    let meeting = store.meeting(&id).unwrap().unwrap();
    let p = Presenter::stored(&meeting.info, &store.transcript(&id).unwrap());

    let m = p.meeting();
    assert_eq!(m["state"], "stopped", "read mode: nothing to stop");
    assert_eq!(m["title"], "Mobile app launch on Dec 1");
    assert_eq!(m["id"], id.as_str());
    assert_eq!(m["elapsed_ms"], meeting.info.duration_ms);
    assert_eq!(m["started_ms"], meeting.info.started_ms);

    assert_eq!(p.transcript(), serde_json::json!({ "count": 12, "open": {} }));
    let first = p.row(1);
    assert_eq!(first["source"], "me");
    assert_eq!(first["speaker"], "Tom");
    assert_eq!(first["speaker_n"], 1);
    // The speaker's id: what a rename from the row's chip names.
    assert_eq!(first["speaker_id"], "me");
    assert_eq!(p.row(2)["speaker_id"], "S1");
    // Where its audio starts (play from here).
    assert_eq!(p.row(2)["start_ms"], store.transcript(&id).unwrap()[1].start_ms);
    assert_eq!(first["open"], false);
    assert!(first["final"].as_str().unwrap().starts_with("Okay, let's get started."));
    assert_eq!(p.row(2)["speaker"], "Priya");
    assert_eq!(p.row(3)["speaker"], "David");
    assert_eq!(p.row(13), serde_json::Value::Null);
    assert_eq!(p.captions(), serde_json::json!([]));
}

#[test]
fn a_meeting_with_only_live_segments_reads_by_side() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = store.create(NOW).unwrap();
    store
        .append_live(
            &info.id,
            &[recap_storage::Segment {
                source: recap_engine::Source::Them,
                speaker: None,
                start_ms: 1_000,
                end_ms: 2_000,
                text: "Hello there.".into(),
            }],
        )
        .unwrap();
    let p = Presenter::stored(&info, &store.transcript(&info.id).unwrap());
    let row = p.row(1);
    assert_eq!(row["source"], "them");
    assert_eq!(row["final"], "Hello there.");
    assert_eq!(row.get("speaker"), None, "the chip says Them");
    assert_eq!(p.meeting()["speakers"], serde_json::json!([]), "no speakers header for a live transcript");
}

fn segment(source: recap_engine::Source, speaker: Option<&str>, start_ms: u64, text: &str) -> recap_storage::Segment {
    recap_storage::Segment {
        source,
        speaker: speaker.map(Into::into),
        start_ms,
        end_ms: start_ms + 2_000,
        text: text.into(),
    }
}

fn pause(at_ms: u64) -> recap_engine::PauseMark {
    recap_engine::PauseMark { at_ms, paused_ms: NOW + at_ms, resumed_ms: NOW + at_ms + 240_000 }
}

#[test]
fn a_stored_meeting_shows_its_pauses_where_they_were() {
    use recap_engine::Source::{Me, Them};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = store.create(NOW).unwrap();
    store
        .append_live(&info.id, &[segment(Them, None, 0, "Before the break."), segment(Them, None, 5_000, "After it.")])
        .unwrap();
    std::fs::write(
        store.folder(&info.id).join(recap::live_meeting::PAUSES),
        serde_json::to_vec(&[pause(4_000), pause(9_000)]).unwrap(),
    )
    .unwrap();
    let pauses = recap::live_meeting::read_pauses(&store.folder(&info.id));
    assert_eq!(pauses.len(), 2);

    // The live transcript: the same side on both sides of a pause is two rows.
    let p = Presenter::stored_with_pauses(&info, &store.transcript(&info.id).unwrap(), &pauses);
    assert_eq!(p.transcript()["count"], 4);
    assert_eq!(p.row(1)["final"], "Before the break.");
    assert_eq!(p.row(2)["pause"], serde_json::json!({ "paused_ms": NOW + 4_000, "resumed_ms": NOW + 244_000 }));
    assert_eq!(p.row(3)["final"], "After it.");
    assert_eq!(p.row(4)["pause"]["paused_ms"], NOW + 9_000, "a pause at the end stays");

    // The refined transcript too, with its speakers in the header.
    store
        .set_refined(
            &info.id,
            &[
                segment(Me, Some("me"), 0, "Let's start."),
                segment(Them, Some("S1"), 1_000, "Sure."),
                segment(Them, Some("S1"), 5_000, "Back again."),
            ],
        )
        .unwrap();
    let info = store
        .update(&info.id, |m| {
            m.speakers = vec![
                recap_storage::Speaker { id: "me".into(), name: None },
                recap_storage::Speaker { id: "S1".into(), name: None },
            ]
        })
        .unwrap();
    let p = Presenter::stored_with_pauses(&info, &store.transcript(&info.id).unwrap(), &pauses[..1]);
    let rows: Vec<_> = (1..=4).map(|i| p.row(i)).collect();
    assert_eq!(rows[0]["speaker"], "Me");
    assert_eq!((rows[1]["speaker"].clone(), rows[1]["final"].clone()), ("Speaker 1".into(), "Sure.".into()));
    assert!(rows[2]["pause"].is_object());
    assert_eq!(rows[3]["final"], "Back again.", "a pause splits a speaker's turn");
    assert_eq!(rows[3]["index"], 4);
    assert_eq!(
        p.meeting()["speakers"],
        serde_json::json!([{ "label": "Me", "n": 1 }, { "label": "Speaker 1", "n": 2 }])
    );
}
