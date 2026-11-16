//! Call detection: what the system says holds the mic, each second, in;
//! hints out. A hint only suggests (a toast); nothing here starts or stops.

use recap_engine::detect::{CallHint, CallWatch, RELEASED_FOR_MS, consent_exe, meeting_app};
use recap_engine::process::{Process, app_root};

fn users(exes: &[&str]) -> Vec<String> {
    exes.iter().map(|e| e.to_string()).collect()
}

#[test]
fn meeting_apps_are_known_by_their_executable() {
    assert_eq!(meeting_app("ms-teams.exe"), Some("Teams"));
    assert_eq!(meeting_app("C:\\Program Files\\Zoom\\bin\\Zoom.exe"), Some("Zoom"));
    assert_eq!(meeting_app("brave.exe"), Some("Brave"));
    assert_eq!(meeting_app("msedge.exe"), Some("Edge"));
    assert_eq!(meeting_app("notepad.exe"), None);
    assert_eq!(meeting_app("recap.exe"), None);
}

#[test]
fn the_consent_store_names_an_app_by_its_path_or_its_package() {
    assert_eq!(consent_exe("C:#Program Files#Zoom#bin#Zoom.exe").as_deref(), Some("zoom.exe"));
    assert_eq!(consent_exe("MSTeams_8wekyb3d8bbwe").as_deref(), Some("ms-teams.exe"));
    assert_eq!(consent_exe("Microsoft.WindowsSoundRecorder_8wekyb3d8bbwe"), None);
}

#[test]
fn a_meeting_app_taking_the_mic_suggests_start_once() {
    let mut watch = CallWatch::default();
    assert_eq!(watch.observe(0, &users(&["notepad.exe"]), false), None);
    assert_eq!(watch.observe(1_000, &users(&["ms-teams.exe"]), false), Some(CallHint::Started { app: "Teams" }));
    // Still on the mic: no second toast.
    assert_eq!(watch.observe(2_000, &users(&["ms-teams.exe"]), false), None);
    assert_eq!(watch.observe(3_000, &users(&["ms-teams.exe", "notepad.exe"]), false), None);
    // Released and taken again: a new call.
    assert_eq!(watch.observe(4_000, &users(&[]), false), None);
    assert_eq!(watch.observe(5_000, &users(&["ms-teams.exe"]), false), Some(CallHint::Started { app: "Teams" }));
}

#[test]
fn nothing_is_suggested_while_notes_are_live() {
    let mut watch = CallWatch::default();
    assert_eq!(watch.observe(0, &users(&["zoom.exe"]), true), None);
    // The meeting ends while Zoom still holds the mic: no toast for it.
    assert_eq!(watch.observe(1_000, &users(&["zoom.exe"]), false), None);
}

#[test]
fn a_mic_free_for_a_minute_during_a_meeting_suggests_stop_once() {
    let mut watch = CallWatch::default();
    assert_eq!(watch.observe(0, &users(&["ms-teams.exe"]), true), None);
    assert_eq!(watch.observe(1_000, &users(&[]), true), None);
    assert_eq!(watch.observe(1_000 + RELEASED_FOR_MS - 1, &users(&[]), true), None);
    assert_eq!(watch.observe(1_000 + RELEASED_FOR_MS, &users(&[]), true), Some(CallHint::Released { app: "Teams" }));
    assert_eq!(watch.observe(2_000 + RELEASED_FOR_MS, &users(&[]), true), None, "once");
}

#[test]
fn the_minute_starts_again_when_the_mic_comes_back() {
    let mut watch = CallWatch::default();
    watch.observe(0, &users(&["zoom.exe"]), true);
    watch.observe(1_000, &users(&[]), true);
    watch.observe(50_000, &users(&["zoom.exe"]), true);
    assert_eq!(watch.observe(51_000, &users(&[]), true), None);
    assert_eq!(watch.observe(51_000 + RELEASED_FOR_MS - 1, &users(&[]), true), None);
    assert_eq!(watch.observe(51_000 + RELEASED_FOR_MS, &users(&[]), true), Some(CallHint::Released { app: "Zoom" }));
}

#[test]
fn notes_without_a_call_app_are_never_offered_stop() {
    let mut watch = CallWatch::default();
    for s in 0..200u64 {
        assert_eq!(watch.observe(s * 1_000, &users(&["notepad.exe"]), true), None);
    }
}

#[test]
fn an_apps_audio_is_its_root_process_tree() {
    // Teams: a root and its helpers; a second, unrelated root started later.
    let list = [
        Process { pid: 4, parent: 0, exe: "System".into() },
        Process { pid: 100, parent: 50, exe: "ms-teams.exe".into() },
        Process { pid: 101, parent: 100, exe: "ms-teams.exe".into() },
        Process { pid: 102, parent: 100, exe: "msedgewebview2.exe".into() },
        Process { pid: 300, parent: 60, exe: "MS-Teams.exe".into() },
    ];
    assert_eq!(app_root(&list, "ms-teams.exe"), Some(100));
    assert_eq!(app_root(&list, "zoom.exe"), None);
}
