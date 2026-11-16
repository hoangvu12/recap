//! Open with Windows: the General page's switch writes and removes the Run
//! value. Tested under a key of its own (`HKCU\Software\Recap-test\...`),
//! never the real Run key, and removed afterwards.
#![cfg(windows)]

use std::{cell::RefCell, rc::Rc};

use recap::{
    autostart::{self, RunKey},
    host::{self, Host},
    user_settings::{PickFolder, SettingsHost},
};
use recap_storage::Store;
use zgx_script::testing::TestApp;

/// A Run key of the test's own, deleted when dropped.
struct TestKey(RunKey);

impl TestKey {
    fn new(name: &str) -> Self {
        Self(RunKey::new(&format!("Software\\Recap-test\\{name}\\Run"), "Recap"))
    }
}

impl Drop for TestKey {
    fn drop(&mut self) {
        self.0.delete_key();
    }
}

#[test]
fn the_command_starts_recap_in_the_tray() {
    assert_eq!(
        autostart::command_line(std::path::Path::new("C:\\Apps\\Recap\\recap.exe")),
        "\"C:\\Apps\\Recap\\recap.exe\" --background"
    );
}

#[test]
fn the_run_value_is_written_and_removed() {
    let key = TestKey::new("roundtrip");
    assert_eq!(key.0.get(), None);
    key.0.set(Some("\"C:\\x\\recap.exe\" --background")).unwrap();
    assert_eq!(key.0.get().as_deref(), Some("\"C:\\x\\recap.exe\" --background"));
    key.0.set(None).unwrap();
    assert_eq!(key.0.get(), None);
    key.0.set(None).unwrap(); // removing what isn't there is fine
}

#[test]
fn the_general_switch_follows_into_the_run_key() {
    let key = TestKey::new("switch");
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_owned();
    let command = "\"C:\\x\\recap.exe\" --background".to_string();
    let made: Rc<RefCell<Option<SettingsHost>>> = Rc::default();
    let (out, run) = (made.clone(), key.0.clone());
    let app = TestApp::new(move |rt| {
        let host = Host::new(rt, host::script_path(), Some(Store::open(&home).unwrap()));
        let pick: PickFolder = Rc::new(|| Box::pin(async { None }));
        let settings = SettingsHost::register(&host.script, home.join("settings.json"), pick);
        settings.list_microphones_with(&host.script, Vec::new);
        autostart::connect(&settings, run, command);
        host.script.shared("window.settings", true);
        *out.borrow_mut() = Some(settings);
        host.script.clone()
    });
    app.frames(2);
    let settings = made.borrow_mut().take().unwrap();
    assert_eq!(key.0.get(), None, "off by default: nothing written");

    let mut win = app.window("settings").unwrap();
    win.click("#nav-general");
    assert!(win.wait_until(|w| w.find("#set-autostart").is_some()));
    win.click("#set-autostart");
    assert!(win.wait_until(|_| settings.current().autostart));
    assert_eq!(key.0.get().as_deref(), Some("\"C:\\x\\recap.exe\" --background"));
    win.click("#set-autostart");
    assert!(win.wait_until(|_| !settings.current().autostart));
    assert_eq!(key.0.get(), None);
}
