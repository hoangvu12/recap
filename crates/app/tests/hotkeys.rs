//! Global hotkeys as Settings has them: registered with the system (a fake
//! keyboard here), changed live, and a hotkey another app holds reported
//! under its row in Settings, Shortcuts.

use std::{cell::RefCell, collections::BTreeMap, path::Path, rc::Rc};

use recap::{
    commands::Command,
    host::{self, Host},
    hotkeys::{HotkeysHost, KeyBoard},
    user_settings::{PickFolder, SettingsHost},
};
use recap_storage::{Settings, Store};
use zgx_script::testing::TestApp;

/// The system's hotkeys: what is registered, by id, and what other apps hold.
#[derive(Default)]
struct Board {
    on: BTreeMap<u32, String>,
    taken: Vec<String>,
    next: u32,
}

#[derive(Clone, Default)]
struct FakeBoard(Rc<RefCell<Board>>);

impl KeyBoard for FakeBoard {
    fn register(&mut self, keys: &str) -> Result<u32, String> {
        let mut b = self.0.borrow_mut();
        if b.taken.iter().any(|k| k == keys) || b.on.values().any(|k| k == keys) {
            return Err("already registered".into());
        }
        b.next += 1;
        let id = b.next;
        b.on.insert(id, keys.to_owned());
        Ok(id)
    }
    fn unregister(&mut self, id: u32) {
        self.0.borrow_mut().on.remove(&id);
    }
}

impl FakeBoard {
    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.0.borrow().on.values().cloned().collect();
        keys.sort();
        keys
    }
    fn id_of(&self, keys: &str) -> u32 {
        *self.0.borrow().on.iter().find(|(_, k)| *k == keys).expect(keys).0
    }
}

struct Fixture {
    _home: tempfile::TempDir,
    _app: TestApp,
    board: FakeBoard,
    hotkeys: HotkeysHost,
    settings: SettingsHost,
}

/// Settings open on Shortcuts; `taken` held by other apps.
fn open(taken: &[&str], before: impl FnOnce(&Path)) -> (Fixture, TestApp) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_owned();
    before(&home);
    let board = FakeBoard::default();
    board.0.borrow_mut().taken = taken.iter().map(|k| k.to_string()).collect();
    let made: Rc<RefCell<Option<(HotkeysHost, SettingsHost)>>> = Rc::default();
    let (out, b) = (made.clone(), board.clone());
    let app = TestApp::new(move |rt| {
        let store = Store::open(&home).unwrap();
        let host = Host::new(rt, host::script_path(), Some(store));
        let pick: PickFolder = Rc::new(|| Box::pin(async { None }));
        let settings = SettingsHost::register(&host.script, home.join("settings.json"), pick);
        settings.list_microphones_with(&host.script, Vec::new);
        let hotkeys = HotkeysHost::connect(&settings, &host.script, rt, Box::new(b));
        host.script.shared("window.settings", true);
        *out.borrow_mut() = Some((hotkeys, settings));
        host.script.clone()
    });
    app.frames(2);
    let (hotkeys, settings) = made.borrow_mut().take().unwrap();
    let mut win = app.window("settings").expect("Settings is open");
    win.click("#nav-shortcuts");
    assert!(win.wait_until(|w| w.find("#page-shortcuts").is_some()));
    (Fixture { _home: dir, _app: app, board, hotkeys, settings }, win)
}

#[test]
fn the_hotkeys_in_settings_are_registered_at_start() {
    let (f, _win) = open(&[], |_| {});
    assert_eq!(f.board.keys(), ["Ctrl+Alt+C", "Ctrl+Alt+N", "Ctrl+Alt+O", "Ctrl+Alt+P"]);
    assert_eq!(f.hotkeys.pressed(f.board.id_of("Ctrl+Alt+N")), Some(Command::ToggleNotes));
    assert_eq!(f.hotkeys.pressed(f.board.id_of("Ctrl+Alt+O")), Some(Command::ToggleCaptions));
    assert_eq!(f.hotkeys.pressed(999), None);
}

#[test]
fn a_hotkey_changed_in_settings_is_registered_again_at_once() {
    let (f, mut win) = open(&[], |_| {});
    win.click("#hotkey-pause");
    assert!(win.wait_until(|w| w.find("#hotkey-pause-recording").is_some()));
    win.keys("ctrl+shift+k");
    assert!(win.wait_until(|_| f.board.keys().contains(&"Ctrl+Shift+K".to_string())));
    assert!(!f.board.keys().contains(&"Ctrl+Alt+P".to_string()), "the old one is let go");
    assert_eq!(f.hotkeys.pressed(f.board.id_of("Ctrl+Shift+K")), Some(Command::PauseResume));

    // Cleared: none for pause.
    f.settings.update(|s| s.hotkeys.pause = String::new());
    assert_eq!(f.board.keys(), ["Ctrl+Alt+C", "Ctrl+Alt+N", "Ctrl+Alt+O"]);
}

#[test]
fn a_hotkey_another_app_holds_is_reported_under_its_row() {
    let (f, mut win) = open(&["Ctrl+Alt+O"], |_| {});
    assert!(win.wait_until(|w| w.find("#hotkey-captions-taken").is_some()));
    assert_eq!(win.text("#hotkey-captions-taken"), "Another app uses Ctrl+Alt+O");
    assert!(win.find("#hotkey-pause-taken").is_none());

    // A free one instead: registered, the report gone.
    win.click("#hotkey-captions");
    assert!(win.wait_until(|w| w.find("#hotkey-captions-recording").is_some()));
    win.keys("ctrl+shift+o");
    assert!(win.wait_until(|_| f.board.keys().contains(&"Ctrl+Shift+O".to_string())));
    assert!(win.wait_until(|w| w.find("#hotkey-captions-taken").is_none()));
}

#[test]
fn swapping_two_hotkeys_registers_both() {
    let (f, _win) = open(&[], |_| {});
    f.settings.update(|s| {
        s.hotkeys.pause = "Ctrl+Alt+O".into();
        s.hotkeys.captions = "Ctrl+Alt+P".into();
    });
    assert_eq!(f.board.keys(), ["Ctrl+Alt+C", "Ctrl+Alt+N", "Ctrl+Alt+O", "Ctrl+Alt+P"]);
    assert_eq!(f.hotkeys.pressed(f.board.id_of("Ctrl+Alt+O")), Some(Command::PauseResume));
    assert_eq!(f.hotkeys.pressed(f.board.id_of("Ctrl+Alt+P")), Some(Command::ToggleCaptions));
}

#[test]
fn hotkeys_rest_while_one_is_being_recorded() {
    // Recording Ctrl+Alt+N must reach Settings, not start notes.
    let (f, mut win) = open(&[], |home| {
        let s = Settings::default();
        s.save(&home.join("settings.json")).unwrap();
    });
    win.click("#hotkey-start_notes");
    assert!(win.wait_until(|_| f.board.keys().is_empty()));
    win.keys("Escape");
    assert!(win.wait_until(|_| f.board.keys().len() == 4));
}
