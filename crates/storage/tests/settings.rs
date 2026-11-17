//! Settings from outside: saved to settings.json and read back, cleaned up
//! when a file holds what the app can't use, and the glossary within what
//! a speech engine takes as keyterms. Audio retention prunes old audio and
//! keeps the transcripts.

use std::fs;

use recap_engine::Source;
use recap_storage::{
    GlossaryError, MeetingAudio, Segment, Settings, Status, Store, Theme,
    settings::{KEYTERM_CHARS, MAX_KEYTERMS},
};

const DAY: u64 = 86_400_000;
const NOV_16: u64 = 1_794_823_200_000;

#[test]
fn without_a_file_the_defaults_show() {
    let dir = tempfile::tempdir().unwrap();
    let s = Settings::load(&dir.path().join("settings.json")).unwrap();
    assert_eq!(s, Settings::default());
    assert_eq!(s.theme, Theme::System);
    assert_eq!(s.caption_size, 20);
    assert!(s.partial_text && s.hide_from_share && s.suggest_on_call);
    assert!(!s.caption_my_speech && !s.autostart);
    assert_eq!(s.meeting_audio, MeetingAudio::System);
    assert_eq!(s.hotkeys.start_notes, "Ctrl+Alt+N");
    assert_eq!(s.keep_audio_days, 0, "audio is kept by default");
}

#[test]
fn saved_settings_read_back_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let mut s = Settings {
        my_name: "Tom".into(),
        theme: Theme::Light,
        caption_size: 24,
        caption_lines: 3,
        partial_text: false,
        microphone: "{0.0.1.00000000}.{headset}".into(),
        meeting_audio: MeetingAudio::App("ms-teams.exe".into()),
        storage_folder: "D:\\Recap".into(),
        keep_audio_days: 30,
        autostart: true,
        ..Settings::default()
    };
    s.add_term("payments API").unwrap();
    s.hotkeys.pause = "Ctrl+Shift+P".into();
    s.save(&path).unwrap();
    assert_eq!(Settings::load(&path).unwrap(), s);
}

#[test]
fn a_file_from_an_older_app_keeps_what_it_has_and_defaults_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "my_name": "Tom", "hotkeys": { "pause": "Ctrl+Alt+K" }, "future_thing": 1 }"#).unwrap();
    let s = Settings::load(&path).unwrap();
    assert_eq!(s.my_name, "Tom");
    assert_eq!(s.hotkeys.pause, "Ctrl+Alt+K");
    assert_eq!(s.hotkeys.start_notes, "Ctrl+Alt+N");
    assert!(s.partial_text);
}

#[test]
fn values_out_of_range_are_brought_back_in() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "caption_size": 400, "caption_lines": 0, "my_name": "  Tom  " }"#).unwrap();
    let s = Settings::load(&path).unwrap();
    assert_eq!(s.caption_size, Settings::CAPTION_SIZES.1);
    assert_eq!(s.caption_lines, 1);
    assert_eq!(s.my_name, "Tom");
}

#[test]
fn a_broken_file_is_an_error_not_silently_reset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "{ not json").unwrap();
    assert!(Settings::load(&path).is_err());
}

#[test]
fn glossary_terms_are_trimmed_unique_and_short() {
    let mut s = Settings::default();
    s.add_term("  Priya ").unwrap();
    assert_eq!(s.add_term("priya"), Err(GlossaryError::Duplicate));
    assert_eq!(s.add_term("   "), Err(GlossaryError::Empty));
    assert_eq!(s.add_term("a term that is far too long"), Err(GlossaryError::TooLong));
    // 20 characters, not bytes: Vietnamese fits.
    s.add_term("Nguyễn Hoàng Vũ Đức!").unwrap();
    assert_eq!(s.glossary, vec!["Priya".to_string(), "Nguyễn Hoàng Vũ Đức!".to_string()]);
    s.remove_term("PRIYA");
    assert_eq!(s.glossary, vec!["Nguyễn Hoàng Vũ Đức!".to_string()]);
}

#[test]
fn the_glossary_holds_at_most_fifty_terms() {
    let mut s = Settings::default();
    for i in 0..MAX_KEYTERMS {
        s.add_term(&format!("term {i}")).unwrap();
    }
    assert_eq!(s.add_term("one more"), Err(GlossaryError::Full));
    assert_eq!(s.keyterms().len(), MAX_KEYTERMS);
}

#[test]
fn glossary_terms_feed_keyterms_within_fifty_by_twenty() {
    // A hand-edited file: too many, too long, repeated, empty.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let mut terms: Vec<String> = vec!["".into(), "Priya".into(), "priya".into(), "x".repeat(21)];
    terms.extend((0..80).map(|i| format!("word {i}")));
    fs::write(&path, serde_json::json!({ "glossary": terms }).to_string()).unwrap();
    let s = Settings::load(&path).unwrap();

    let keyterms = s.keyterms();
    assert_eq!(keyterms.len(), MAX_KEYTERMS);
    assert!(keyterms.iter().all(|t| !t.is_empty() && t.chars().count() <= KEYTERM_CHARS));
    assert_eq!(keyterms[0], "Priya");
    assert_eq!(keyterms[1], "word 0");
    assert_eq!(s.live_options().keyterms, keyterms);
}

#[test]
fn keyterms_add_attendees_after_the_glossary_within_the_limit() {
    let mut s = Settings::default();
    s.add_term("payments API").unwrap();
    let extra: Vec<String> = ["Priya", "payments api", "David"].map(String::from).into();
    assert_eq!(s.keyterms_with(&extra), ["payments API", "Priya", "David"]);
    for i in 0..MAX_KEYTERMS {
        let _ = s.add_term(&format!("t{i}"));
    }
    assert_eq!(s.keyterms_with(&extra).len(), MAX_KEYTERMS);
}

#[test]
fn hotkeys_are_written_one_way_and_need_a_modifier() {
    use recap_storage::settings::hotkey;
    assert_eq!(hotkey("ctrl+alt+n").as_deref(), Some("Ctrl+Alt+N"));
    assert_eq!(hotkey("Shift+Ctrl+p").as_deref(), Some("Ctrl+Shift+P"));
    assert_eq!(hotkey("win+alt+f5").as_deref(), Some("Alt+Win+F5"));
    assert_eq!(hotkey("F9").as_deref(), Some("F9"));
    assert_eq!(hotkey("n"), None, "a plain letter would eat typing");
    assert_eq!(hotkey("Ctrl+Alt"), None, "no key");
    assert_eq!(hotkey("Ctrl+Alt+N+M"), None);
}

#[test]
fn the_meetings_folder_is_the_storage_folder_or_home() {
    let mut s = Settings::default();
    let home = std::path::Path::new("C:\\Users\\me\\AppData\\Local\\Recap");
    assert_eq!(s.meetings_root(home), home);
    s.storage_folder = "D:\\Notes".into();
    assert_eq!(s.meetings_root(home), std::path::Path::new("D:\\Notes"));
}

// Audio retention ----------------------------------------------------------

fn meeting_with_audio(store: &Store, started_ms: u64, status: Status) -> recap_storage::MeetingId {
    let info = store.create(started_ms).unwrap();
    store
        .append_live(
            &info.id,
            &[Segment { source: Source::Them, speaker: None, start_ms: 0, end_ms: 900, text: "Hi Tom".into() }],
        )
        .unwrap();
    store.update(&info.id, |i| i.status = status).unwrap();
    let folder = store.folder(&info.id);
    fs::write(folder.join("audio.ogg"), b"OggS").unwrap();
    fs::write(folder.join("stt_them.json"), b"{}").unwrap();
    info.id
}

#[test]
fn pruning_deletes_old_audio_and_keeps_transcripts() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let old = meeting_with_audio(&store, NOV_16, Status::Done);
    let recent = meeting_with_audio(&store, NOV_16 + 25 * DAY, Status::Done);
    let unfinished = meeting_with_audio(&store, NOV_16 + DAY, Status::Processing);
    let now = NOV_16 + 31 * DAY;

    let pruned = store.prune_audio(now, 30).unwrap();
    assert_eq!(pruned, vec![old.clone()]);
    assert!(!store.folder(&old).join("audio.ogg").exists());
    assert!(store.folder(&old).join("stt_them.json").exists());
    assert_eq!(store.transcript(&old).unwrap().len(), 1, "the transcript stays");
    assert!(store.meeting(&old).unwrap().is_some());
    assert!(store.folder(&recent).join("audio.ogg").exists());
    assert!(store.folder(&unfinished).join("audio.ogg").exists(), "Recover still needs it");
    // Again: nothing left to prune.
    assert!(store.prune_audio(now, 30).unwrap().is_empty());
}

#[test]
fn keeping_audio_forever_prunes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let old = meeting_with_audio(&store, NOV_16, Status::Done);
    assert!(store.prune_audio(NOV_16 + 1000 * DAY, 0).unwrap().is_empty());
    assert!(store.folder(&old).join("audio.ogg").exists());
}
