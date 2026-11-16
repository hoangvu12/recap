//! The store from outside: write meetings, read them back, lose the
//! database and get the same rows from the folders.

use std::fs;

use recap_engine::Source;
use recap_storage::{Segment, Speaker, Status, Store, Summary, TranscriptKind};
use serde_json::json;

const NOV_16_10AM: u64 = 1_794_823_200_000;

fn seg(source: Source, speaker: Option<&str>, start_ms: u64, text: &str) -> Segment {
    Segment { source, speaker: speaker.map(Into::into), start_ms, end_ms: start_ms + 2_000, text: text.into() }
}

fn summary() -> Summary {
    Summary {
        overview: "Tom, Priya and David agreed to launch on December 1.".into(),
        markdown: "## Overview\nLaunch on December 1.\n".into(),
        data: json!({ "title": "Mobile app launch on Dec 1", "next_steps": [{ "owner": "David", "text": "Add caching" }] }),
    }
}

/// A whole meeting: live while it ran, then refined, named and summarised.
fn write_meeting(store: &Store) -> recap_storage::MeetingId {
    let info = store.create(NOV_16_10AM).unwrap();
    store
        .append_live(&info.id, &[seg(Source::Me, None, 0, "Okay, let's get started."), seg(Source::Them, None, 3_000, "Thanks, Tom.")])
        .unwrap();
    store.append_live(&info.id, &[seg(Source::Them, None, 6_000, "The payments API is ready.")]).unwrap();
    store.set_my_notes(&info.id, "Ask about the India fallback").unwrap();
    store
        .set_refined(
            &info.id,
            &[
                seg(Source::Me, Some("me"), 0, "Okay, let's get started."),
                seg(Source::Them, Some("S1"), 3_000, "Thanks, Tom."),
                seg(Source::Them, Some("S2"), 6_000, "The payments API is ready."),
            ],
        )
        .unwrap();
    store
        .update(&info.id, |m| {
            m.title = "Mobile app launch on Dec 1".into();
            m.duration_ms = 118_000;
            m.status = Status::Done;
            m.speakers = vec![
                Speaker { id: "me".into(), name: Some("Tom".into()) },
                Speaker { id: "S1".into(), name: Some("Priya".into()) },
                Speaker { id: "S2".into(), name: None },
            ];
        })
        .unwrap();
    store.set_summary(&info.id, &summary()).unwrap();
    info.id
}

#[test]
fn a_new_store_makes_its_database_and_meetings_folder() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert!(dir.path().join("app.db").is_file());
    assert!(dir.path().join("meetings").is_dir());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn the_database_runs_in_wal_mode_with_every_table() {
    let dir = tempfile::tempdir().unwrap();
    let _store = Store::open(dir.path()).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("app.db")).unwrap();
    let mode: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    for table in ["meetings", "segments", "speakers", "people", "notes", "keyterms", "providers", "settings", "search"] {
        let n: i64 = db
            .query_row("SELECT count(*) FROM sqlite_master WHERE name = ?1", [table], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "table {table}");
    }
}

#[test]
fn a_new_meeting_is_recording_and_has_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = store.create(NOV_16_10AM).unwrap();
    assert_eq!(info.status, Status::Recording);
    assert_eq!(info.started_ms, NOV_16_10AM);
    assert_eq!(info.title, "");
    assert!(store.folder(&info.id).is_dir());
    assert_eq!(store.list().unwrap(), vec![info]);
}

#[test]
fn two_meetings_in_the_same_second_get_their_own_folders() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let a = store.create(NOV_16_10AM).unwrap();
    let b = store.create(NOV_16_10AM + 10).unwrap();
    assert_ne!(a.id, b.id);
    assert_ne!(store.folder(&a.id), store.folder(&b.id));
}

#[test]
fn the_list_is_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let old = store.create(NOV_16_10AM - 86_400_000).unwrap();
    let new = store.create(NOV_16_10AM).unwrap();
    let ids: Vec<_> = store.list().unwrap().into_iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![new.id, old.id]);
}

#[test]
fn a_meeting_reads_back_whole() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = write_meeting(&store);

    let meeting = store.meeting(&id).unwrap().unwrap();
    assert_eq!(meeting.info.title, "Mobile app launch on Dec 1");
    assert_eq!(meeting.info.duration_ms, 118_000);
    assert_eq!(meeting.info.status, Status::Done);
    assert_eq!(meeting.info.speakers.len(), 3);
    assert_eq!(meeting.info.speakers[1].name.as_deref(), Some("Priya"));
    assert_eq!(meeting.summary, Some(summary()));
    assert_eq!(meeting.my_notes, "Ask about the India fallback");

    let live = store.segments(&id, TranscriptKind::Live).unwrap();
    assert_eq!(live.len(), 3);
    assert_eq!(live[2].text, "The payments API is ready.");
    let refined = store.segments(&id, TranscriptKind::Refined).unwrap();
    assert_eq!(refined[1].speaker.as_deref(), Some("S1"));
}

#[test]
fn the_meeting_folder_holds_live_refined_and_summary_files() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = write_meeting(&store);
    let folder = store.folder(&id);

    let live = fs::read_to_string(folder.join("live.ndjson")).unwrap();
    assert_eq!(live.lines().count(), 3);
    for line in live.lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("one JSON object a line");
    }
    assert_eq!(fs::read_to_string(folder.join("refined.ndjson")).unwrap().lines().count(), 3);
    assert_eq!(fs::read_to_string(folder.join("summary.md")).unwrap(), summary().markdown);
}

#[test]
fn refined_segments_replace_the_last_refined_ones() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = write_meeting(&store);
    store.set_refined(&id, &[seg(Source::Them, Some("S1"), 0, "Again.")]).unwrap();
    assert_eq!(store.segments(&id, TranscriptKind::Refined).unwrap().len(), 1);
    assert_eq!(fs::read_to_string(store.folder(&id).join("refined.ndjson")).unwrap().lines().count(), 1);
}

#[test]
fn the_transcript_is_the_refined_one_once_there_is_one() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = store.create(NOV_16_10AM).unwrap();
    store.append_live(&info.id, &[seg(Source::Them, None, 0, "live words")]).unwrap();
    assert_eq!(store.transcript(&info.id).unwrap()[0].text, "live words");
    store.set_refined(&info.id, &[seg(Source::Them, Some("S1"), 0, "refined words")]).unwrap();
    assert_eq!(store.transcript(&info.id).unwrap()[0].text, "refined words");
}

#[test]
fn round_trip_delete_the_database_and_rebuild_from_the_folders() {
    let dir = tempfile::tempdir().unwrap();
    let (before, list, live, refined) = {
        let store = Store::open(dir.path()).unwrap();
        let id = write_meeting(&store);
        store.create(NOV_16_10AM + 3_600_000).unwrap();
        (
            store.meeting(&id).unwrap().unwrap(),
            store.list().unwrap(),
            store.segments(&id, TranscriptKind::Live).unwrap(),
            store.segments(&id, TranscriptKind::Refined).unwrap(),
        )
    };
    for file in ["app.db", "app.db-wal", "app.db-shm"] {
        let _ = fs::remove_file(dir.path().join(file));
    }
    assert!(!dir.path().join("app.db").exists());

    let store = Store::open(dir.path()).unwrap();
    let id = before.info.id.clone();
    assert_eq!(store.list().unwrap(), list);
    assert_eq!(store.meeting(&id).unwrap().unwrap(), before);
    assert_eq!(store.segments(&id, TranscriptKind::Live).unwrap(), live);
    assert_eq!(store.segments(&id, TranscriptKind::Refined).unwrap(), refined);
    assert_eq!(store.search("payments").unwrap(), vec![id]);
}

#[test]
fn rebuild_on_an_open_store_gives_the_same_rows() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = write_meeting(&store);
    let before = (store.list().unwrap(), store.meeting(&id).unwrap(), store.transcript(&id).unwrap());
    store.rebuild().unwrap();
    assert_eq!((store.list().unwrap(), store.meeting(&id).unwrap(), store.transcript(&id).unwrap()), before);
}

#[test]
fn a_live_file_cut_off_mid_line_rebuilds_up_to_the_cut() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let store = Store::open(dir.path()).unwrap();
        let info = store.create(NOV_16_10AM).unwrap();
        store.append_live(&info.id, &[seg(Source::Them, None, 0, "first"), seg(Source::Them, None, 3_000, "second")]).unwrap();
        let path = store.folder(&info.id).join("live.ndjson");
        let mut text = fs::read_to_string(&path).unwrap();
        text.truncate(text.len() - 10); // the app died while writing "second"
        fs::write(&path, text).unwrap();
        info.id
    };
    fs::remove_file(dir.path().join("app.db")).unwrap();
    let store = Store::open(dir.path()).unwrap();
    let live = store.segments(&id, TranscriptKind::Live).unwrap();
    assert_eq!(live.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["first"]);
}

#[test]
fn search_finds_titles_people_and_transcript_words() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = write_meeting(&store);
    let other = store.create(NOV_16_10AM + 1).unwrap();
    store.append_live(&other.id, &[seg(Source::Them, None, 0, "Quarterly budget talk")]).unwrap();

    assert_eq!(store.search("launch").unwrap(), vec![id.clone()], "title");
    assert_eq!(store.search("priya").unwrap(), vec![id.clone()], "speaker name");
    assert_eq!(store.search("payme").unwrap(), vec![id.clone()], "a word's start");
    assert_eq!(store.search("budget").unwrap(), vec![other.id.clone()], "live words");
    assert!(store.search("\"unbalanced").unwrap().is_empty(), "odd input is not an error");
    assert!(store.search("   ").unwrap().is_empty());
}

#[test]
fn a_deleted_meeting_is_gone_with_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = write_meeting(&store);
    let folder = store.folder(&id);
    store.delete(&id).unwrap();
    assert!(!folder.exists());
    assert!(store.list().unwrap().is_empty());
    assert!(store.meeting(&id).unwrap().is_none());
    assert!(store.search("payments").unwrap().is_empty());
}

#[test]
fn writing_to_an_unknown_meeting_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recap_storage::MeetingId::from("nope");
    assert!(store.append_live(&id, &[seg(Source::Me, None, 0, "x")]).is_err());
    assert!(store.update(&id, |m| m.title = "x".into()).is_err());
}

#[test]
fn the_store_is_shared_across_threads() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = store.create(NOV_16_10AM).unwrap();
    let writer = {
        let (store, id) = (store.clone(), info.id.clone());
        std::thread::spawn(move || {
            for i in 0..20 {
                store.append_live(&id, &[seg(Source::Them, None, i * 1_000, "word")]).unwrap();
            }
        })
    };
    writer.join().unwrap();
    assert_eq!(store.segments(&info.id, TranscriptKind::Live).unwrap().len(), 20);
}
