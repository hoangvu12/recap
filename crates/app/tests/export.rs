//! Copy as Markdown / Export .md, Show recording in folder, Home's Recover
//! row and search, from a store seeded with the test meeting.

use recap::{export, home, seed};
use recap_engine::Source;
use recap_storage::{MeetingId, Segment, Speaker, Status, Store};

const NOW: u64 = 1_794_830_400_000;
const DATE: &str = "Mon, Nov 16, 10:00 AM";

fn seeded() -> (tempfile::TempDir, Store, MeetingId) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = seed::test_meeting(&store, NOW).unwrap().id;
    (dir, store, id)
}

#[test]
fn the_markdown_has_the_title_facts_summary_and_named_transcript() {
    let (_dir, store, id) = seeded();
    let md = export::meeting_markdown(&store, &id, DATE).unwrap();

    assert!(md.starts_with("# Mobile app launch on Dec 1\n\n"), "{md}");
    assert!(md.contains(&format!("**Date:** {DATE}")));
    assert!(md.contains("**Duration:** 1:5") || md.contains("**Duration:** 2:0"), "{md}");
    assert!(md.contains("**Speakers:** Tom, Priya, David"));

    // The summary as the Summary tab has it, under its own heading.
    let summary = md.find("## Summary\n").expect("a summary section");
    assert!(md[summary..].contains("India-only soft launch is the fallback"));
    assert!(md.contains("### Next steps\n- [ ] **David**: Add caching"));

    // Then the transcript, each line by its speaker's name and time.
    let transcript = md.find("## Transcript\n").expect("a transcript section");
    assert!(transcript > summary);
    let lines: Vec<&str> = md[transcript..].lines().filter(|l| l.starts_with("**")).collect();
    assert_eq!(lines.len(), 12);
    assert!(lines[0].starts_with("**Tom** (0:0") && lines[0].contains("): Okay, let's get started."), "{}", lines[0]);
    assert!(lines[1].starts_with("**Priya** ("));
    assert!(lines[2].starts_with("**David** ("));
    assert!(md.ends_with('\n'));
}

#[test]
fn a_meeting_without_summary_or_names_exports_its_live_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = store.create(NOW).unwrap();
    let seg = |source, start_ms, text: &str| Segment { source, speaker: None, start_ms, end_ms: start_ms + 1_000, text: text.into() };
    store.append_live(&info.id, &[seg(Source::Me, 0, "Hi all."), seg(Source::Them, 65_000, "Hello.")]).unwrap();
    store.set_my_notes(&info.id, "Ask about Dec 1").unwrap();

    let md = export::meeting_markdown(&store, &info.id, DATE).unwrap();
    assert!(md.starts_with("# Untitled meeting\n\n"), "{md}");
    assert!(!md.contains("## Summary"));
    assert!(!md.contains("**Speakers:**"));
    assert!(md.contains("## My notes\n\nAsk about Dec 1\n"));
    assert!(md.contains("**Me** (0:00): Hi all.\n"));
    assert!(md.contains("**Them** (1:05): Hello.\n"));
}

#[test]
fn unnamed_speakers_are_speaker_n() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = store.create(NOW).unwrap();
    store
        .set_refined(
            &info.id,
            &[Segment { source: Source::Them, speaker: Some("S2".into()), start_ms: 3_725_000, end_ms: 3_726_000, text: "Late.".into() }],
        )
        .unwrap();
    store.update(&info.id, |m| m.speakers = vec![Speaker { id: "S2".into(), name: None }]).unwrap();
    let md = export::meeting_markdown(&store, &info.id, DATE).unwrap();
    assert!(md.contains("**Speakers:** Speaker 2"));
    assert!(md.contains("**Speaker 2** (1:02:05): Late.\n"), "{md}");
}

#[test]
fn an_unknown_meeting_has_no_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert!(export::meeting_markdown(&store, &MeetingId::from("nope"), DATE).is_err());
}

#[test]
fn the_file_name_is_the_title_made_safe() {
    let (_dir, store, id) = seeded();
    let info = store.meeting(&id).unwrap().unwrap().info;
    assert_eq!(export::file_name(&info), "Mobile app launch on Dec 1.md");

    let mut odd = info.clone();
    odd.title = "Q3: plan/budget? <draft> ".into();
    assert_eq!(export::file_name(&odd), "Q3 plan budget draft.md");
    odd.title = "  ...".into();
    assert_eq!(export::file_name(&odd), "Meeting.md");
}

#[test]
fn show_in_folder_selects_the_recording_else_opens_the_folder() {
    let (_dir, store, id) = seeded();
    assert_eq!(export::reveal_target(&store, &id), export::Reveal::Open(store.folder(&id)));
    std::fs::write(store.recording(&id), b"OggS").unwrap();
    assert_eq!(export::reveal_target(&store, &id), export::Reveal::Select(store.recording(&id)));
}

#[test]
fn home_offers_recover_only_for_an_unfinished_meeting_with_a_recording() {
    let (_dir, store, done) = seeded();
    let crashed = store.create(NOW - 3_600_000).unwrap();
    std::fs::write(store.recording(&crashed.id), b"OggS").unwrap();
    let empty = store.create(NOW - 7_200_000).unwrap();
    assert_eq!(store.meeting(&done).unwrap().unwrap().info.status, Status::Done);

    let list = home::meetings(&store, &store.list().unwrap());
    let recover = |id: &MeetingId| {
        list.as_array().unwrap().iter().find(|m| m["id"] == id.as_str()).unwrap()["recover"].as_bool().unwrap()
    };
    assert!(!recover(&done), "done: nothing to recover");
    assert!(recover(&crashed.id), "unfinished with its recording");
    assert!(!recover(&empty.id), "unfinished, nothing recorded");
}

#[test]
fn home_search_finds_the_test_meeting_by_dec_1_and_by_people() {
    let (_dir, store, id) = seeded();
    let other = store.create(NOW - 86_400_000).unwrap();
    store
        .append_live(&other.id, &[Segment { source: Source::Them, speaker: None, start_ms: 0, end_ms: 1_000, text: "Budget review.".into() }])
        .unwrap();
    assert_eq!(home::search(&store, "Dec 1"), serde_json::json!([id.as_str()]));
    assert_eq!(home::search(&store, "priya caching"), serde_json::json!([id.as_str()]));
    assert_eq!(home::search(&store, "budget"), serde_json::json!([other.id.as_str()]));
    assert_eq!(home::search(&store, "nothing like this"), serde_json::json!([]));
}
