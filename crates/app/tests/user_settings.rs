//! The Settings pages but Providers, through the real UI (ui/app.luau,
//! headless) and the real host side (recap::user_settings): what the user
//! changes lands in settings.json, reaches whoever listens, and takes
//! effect (the storage folder, audio retention).

use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use recap::{
    host::{self, Host},
    user_settings::{PickFolder, SettingsHost},
};
use recap_storage::{MeetingAudio, Settings, Status, Store, Theme};
use zgx_script::testing::TestApp;

const DAY: u64 = 86_400_000;
const NOW: u64 = 1_794_830_400_000;

struct Fixture {
    _home: tempfile::TempDir,
    home: PathBuf,
    app: TestApp,
    host: Host,
    settings: SettingsHost,
}

impl Fixture {
    fn saved(&self) -> Settings {
        Settings::load(&self.home.join("settings.json")).unwrap()
    }
}

/// Settings open on `page`, Recap's home a fresh folder; Change folder
/// picks `picked`.
fn open(page: &str, before: impl FnOnce(&Path), picked: Option<PathBuf>) -> (Fixture, TestApp) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_owned();
    before(&home);
    let made: Rc<RefCell<Option<(Host, SettingsHost)>>> = Rc::default();
    let out = made.clone();
    let h = home.clone();
    let app = TestApp::new(move |rt| {
        let settings_path = h.join("settings.json");
        let settings = Settings::load(&settings_path).unwrap();
        let store = Store::open(&settings.meetings_root(&h)).unwrap();
        let host = Host::new(rt, host::script_path(), Some(store));
        let pick: PickFolder = Rc::new(move || {
            let picked = picked.clone();
            Box::pin(async move { picked })
        });
        let settings = SettingsHost::register(&host.script, settings_path, pick);
        settings.connect_store(&host, h.clone(), || NOW);
        host.script.shared("window.settings", true);
        *out.borrow_mut() = Some((host.clone(), settings));
        host.script.clone()
    });
    app.frames(2);
    let (host, settings) = made.borrow_mut().take().unwrap();
    let mut win = app.window("settings").expect("Settings is open");
    win.click(&format!("#nav-{page}"));
    assert!(win.wait_until(|w| w.find(&format!("#page-{page}")).is_some()), "page {page}");
    (Fixture { _home: dir, home, app, host, settings }, win)
}

#[test]
fn a_switch_saves_and_tells_whoever_listens() {
    let (f, mut win) = open("captions", |_| {}, None);
    let mut watch = f.settings.subscribe();
    assert!(f.saved().partial_text);
    win.click("#set-partial_text");
    assert!(win.wait_until(|_| !f.settings.current().partial_text));
    assert!(!f.saved().partial_text);
    assert!(watch.has_changed().unwrap());
    assert!(!watch.borrow_and_update().partial_text);
    // The switch shows it.
    assert!(win.wait_until(|w| w.text("#set-partial_text-state") == "off"));
    drop(f.app);
}

#[test]
fn my_name_is_saved_when_typed() {
    let (f, mut win) = open("general", |_| {}, None);
    win.type_text("#set-my_name", "Tom");
    win.keys("Enter");
    assert!(win.wait_until(|_| f.saved().my_name == "Tom"));
}

#[test]
fn the_theme_is_picked_from_its_menu() {
    let (f, mut win) = open("general", |_| {}, None);
    assert_eq!(win.text("#set-theme-label"), "Follow system");
    win.click("#set-theme");
    assert!(win.wait_until(|w| w.find("#set-theme-light").is_some()));
    win.click("#set-theme-light");
    assert!(win.wait_until(|_| f.saved().theme == Theme::Light));
    assert!(win.wait_until(|w| w.text("#set-theme-label") == "Light"));
}

#[test]
fn caption_size_steps_within_its_range() {
    let (f, mut win) = open(
        "captions",
        |home| {
            let s = Settings { caption_size: 38, ..Settings::default() };
            s.save(&home.join("settings.json")).unwrap();
        },
        None,
    );
    assert_eq!(win.text("#set-caption_size-value"), "38");
    win.click("#set-caption_size-plus");
    assert!(win.wait_until(|_| f.saved().caption_size == 40));
    win.click("#set-caption_size-plus");
    win.click("#set-caption_size-minus");
    assert!(win.wait_until(|_| f.saved().caption_size == 38));
    win.click("#set-caption_lines-plus");
    assert!(win.wait_until(|_| f.saved().caption_lines == 3));
}

#[test]
fn meeting_audio_can_be_one_app() {
    let (f, mut win) = open("audio", |_| {}, None);
    win.click("#set-meeting_audio");
    assert!(win.wait_until(|w| w.find("#set-meeting_audio-ms-teams.exe").is_some()));
    win.click("#set-meeting_audio-ms-teams.exe");
    assert!(win.wait_until(|_| f.saved().meeting_audio == MeetingAudio::App("ms-teams.exe".into())));
}

#[test]
fn glossary_terms_typed_become_keyterms() {
    let (f, mut win) = open("glossary", |_| {}, None);
    win.type_text("#glossary-input", "payments API");
    win.keys("Enter");
    assert!(win.wait_until(|_| f.saved().glossary == ["payments API"]));
    assert_eq!(f.settings.current().keyterms(), ["payments API"]);
    assert!(win.wait_until(|w| w.find("#term-1").is_some()));
    assert_eq!(win.text("#term-1"), "payments API");

    // A term no engine takes is refused, and says why.
    win.type_text("#glossary-input", "a much too long glossary term");
    win.keys("Enter");
    assert!(win.wait_until(|w| w.find("#glossary-error").is_some()));
    assert_eq!(f.saved().glossary.len(), 1);

    win.click("#term-1-remove");
    assert!(win.wait_until(|_| f.saved().glossary.is_empty()));
}

#[test]
fn a_hotkey_is_recorded_from_the_keys_pressed() {
    let (f, mut win) = open("shortcuts", |_| {}, None);
    assert_eq!(win.text("#hotkey-start_notes-keys"), "Ctrl+Alt+N");
    win.click("#hotkey-pause");
    assert!(win.wait_until(|w| w.find("#hotkey-pause-recording").is_some()));
    win.keys("ctrl+shift+k");
    assert!(win.wait_until(|_| f.saved().hotkeys.pause == "Ctrl+Shift+K"));

    // One already in use is refused.
    win.click("#hotkey-captions");
    win.keys("ctrl+shift+k");
    assert!(win.wait_until(|w| w.find("#hotkey-captions-error").is_some()));
    assert_eq!(f.saved().hotkeys.captions, "Ctrl+Alt+O");
}

fn meeting_with_audio(store: &Store, started_ms: u64) -> recap_storage::MeetingId {
    let info = store.create(started_ms).unwrap();
    store.update(&info.id, |i| i.status = Status::Done).unwrap();
    fs::write(store.folder(&info.id).join("audio.ogg"), b"OggS").unwrap();
    info.id
}

#[test]
fn keeping_audio_for_30_days_deletes_older_audio_now() {
    let mut old = None;
    let (f, mut win) = open(
        "storage",
        |home| {
            let store = Store::open(home).unwrap();
            old = Some(meeting_with_audio(&store, NOW - 40 * DAY));
        },
        None,
    );
    let old = old.unwrap();
    let store = f.host.store().unwrap();
    assert!(store.folder(&old).join("audio.ogg").exists(), "kept by default");
    win.click("#set-keep_audio_days");
    assert!(win.wait_until(|w| w.find("#set-keep_audio_days-30").is_some()));
    win.click("#set-keep_audio_days-30");
    assert!(win.wait_until(|_| f.saved().keep_audio_days == 30));
    assert!(win.wait_until(|_| !store.folder(&old).join("audio.ogg").exists()));
    assert!(store.meeting(&old).unwrap().is_some(), "the meeting stays");
}

#[test]
fn changing_the_storage_folder_shows_the_meetings_there() {
    let other = tempfile::tempdir().unwrap();
    let there = Store::open(other.path()).unwrap();
    meeting_with_audio(&there, NOW - DAY);
    let (f, mut win) = open("storage", |_| {}, Some(other.path().to_owned()));
    assert!(f.host.store().unwrap().list().unwrap().is_empty());
    win.click("#storage-change");
    assert!(win.wait_until(|_| f.saved().storage_folder == other.path().to_string_lossy()));
    assert!(win.wait_until(|_| f.host.store().is_some_and(|s| s.list().unwrap().len() == 1)));
    assert!(win.wait_until(|w| w.text("#storage-folder") == other.path().to_string_lossy()));
}
