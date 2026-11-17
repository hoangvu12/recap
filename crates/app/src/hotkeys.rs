//! Global hotkeys (Settings, Shortcuts) registered with the system, kept as
//! Settings has them: registered at start, again when they change, resting
//! while Settings records a new one (so its keys reach Settings). One the
//! system refuses (another app holds it) is reported under its row.
//!
//! ```text
//! shared   hotkey_conflicts   { [id] = "Another app uses Ctrl+Alt+O" }
//! reads    hotkey_recording   the hotkey Settings is recording ("" none)
//! ```
//!
//! The system's side is a [`KeyBoard`]: [`SystemKeys`] on Windows
//! (`global-hotkey`, RegisterHotKey), a fake in tests.

use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use recap_storage::{Hotkeys, Settings};
use zgx_script::{ScriptHost, Shared, Value};

use crate::{commands::Command, user_settings::SettingsHost};

/// Where hotkeys are registered.
pub trait KeyBoard {
    /// Register `keys` (`Ctrl+Alt+N`): the id its presses come with, or why
    /// not (another app holds it).
    fn register(&mut self, keys: &str) -> Result<u32, String>;
    fn unregister(&mut self, id: u32);
}

/// One registered hotkey.
struct Active {
    id: &'static str,
    keys: String,
    os_id: u32,
}

/// The hotkeys registered now, and those refused.
pub struct Registered {
    board: Box<dyn KeyBoard>,
    active: Vec<Active>,
    refused: BTreeMap<String, String>,
}

const IDS: [&str; 4] = ["start_notes", "pause", "click_through", "captions"];

impl Registered {
    pub fn new(board: Box<dyn KeyBoard>) -> Self {
        Self { board, active: Vec::new(), refused: BTreeMap::new() }
    }

    /// Register `wanted` (none while `resting`): what changed is let go
    /// first, then taken, so two hotkeys can swap keys. The refused ones,
    /// by id: what to say.
    pub fn apply(&mut self, wanted: &Hotkeys, resting: bool) -> BTreeMap<String, String> {
        let keys_of = |id: &str| -> String {
            if resting {
                return String::new();
            }
            wanted.all().iter().find(|(k, _)| *k == id).map(|(_, v)| v.to_string()).unwrap_or_default()
        };
        let board = &mut self.board;
        self.active.retain(|a| {
            let keep = keys_of(a.id) == a.keys;
            if !keep {
                board.unregister(a.os_id);
            }
            keep
        });
        self.refused.clear();
        for id in IDS {
            let keys = keys_of(id);
            if keys.is_empty() || self.active.iter().any(|a| a.id == id) {
                continue;
            }
            match self.board.register(&keys) {
                Ok(os_id) => self.active.push(Active { id, keys, os_id }),
                Err(error) => {
                    eprintln!("recap: hotkey {keys}: {error}");
                    self.refused.insert(id.to_owned(), format!("Another app uses {keys}"));
                }
            }
        }
        self.refused.clone()
    }

    /// The hotkey whose press came with `os_id`.
    pub fn pressed(&self, os_id: u32) -> Option<&'static str> {
        self.active.iter().find(|a| a.os_id == os_id).map(|a| a.id)
    }

    /// Let every hotkey go (the app quits).
    pub fn clear(&mut self) {
        for a in self.active.drain(..) {
            self.board.unregister(a.os_id);
        }
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        self.clear();
    }
}

/// The hotkeys of [`SettingsHost`], registered on a [`KeyBoard`] and kept
/// as they change.
#[derive(Clone)]
pub struct HotkeysHost {
    registered: Rc<RefCell<Registered>>,
    _watch: Rc<zgx::reactive::Effect>,
}

fn value(json: serde_json::Value) -> Value {
    Value::from_json(&json.to_string()).unwrap_or(Value::Nil)
}

impl HotkeysHost {
    pub fn connect(
        settings: &SettingsHost,
        script: &ScriptHost,
        runtime: &zgx::reactive::Runtime,
        board: Box<dyn KeyBoard>,
    ) -> Self {
        let registered = Rc::new(RefCell::new(Registered::new(board)));
        let conflicts = script.shared("hotkey_conflicts", value(serde_json::json!({})));
        let recording = script.shared("hotkey_recording", Value::Str("".into()));
        let apply: Rc<dyn Fn(&Settings)> = {
            let (registered, conflicts, recording) = (registered.clone(), conflicts.clone(), recording.clone());
            Rc::new(move |s: &Settings| {
                let resting = recording.with_untracked(|v| matches!(v, Value::Str(id) if !id.is_empty()));
                publish(&conflicts, registered.borrow_mut().apply(&s.hotkeys, resting));
            })
        };
        let a = apply.clone();
        settings.on_change(move |old, new| {
            if old.hotkeys != new.hotkeys {
                a(new);
            }
        });
        // Registered now, and again as Settings starts or stops recording.
        let (a, s) = (apply, settings.clone());
        let watch = runtime.effect(move || {
            recording.with(|_| ());
            let current = s.current();
            a(&current);
        });
        Self { registered, _watch: Rc::new(watch) }
    }

    /// The command of the hotkey pressed (its id, as the system gave it).
    pub fn pressed(&self, os_id: u32) -> Option<Command> {
        self.registered.borrow().pressed(os_id).and_then(Command::for_hotkey)
    }

    /// Let every hotkey go.
    pub fn clear(&self) {
        self.registered.borrow_mut().clear();
    }
}

fn publish(shared: &Shared, refused: BTreeMap<String, String>) {
    shared.set(value(serde_json::to_value(refused).unwrap_or_default()));
}

/// The system's hotkeys (Windows: RegisterHotKey through `global-hotkey`).
/// Make it on the UI thread: presses arrive through its message loop.
#[cfg(windows)]
pub struct SystemKeys {
    manager: global_hotkey::GlobalHotKeyManager,
    keys: BTreeMap<u32, global_hotkey::hotkey::HotKey>,
}

#[cfg(windows)]
impl SystemKeys {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            manager: global_hotkey::GlobalHotKeyManager::new().map_err(|e| e.to_string())?,
            keys: BTreeMap::new(),
        })
    }

    /// Presses, as they come: their ids. Call once.
    pub fn presses() -> tokio::sync::mpsc::UnboundedReceiver<u32> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(move |e: global_hotkey::GlobalHotKeyEvent| {
            if e.state == global_hotkey::HotKeyState::Pressed {
                let _ = tx.send(e.id);
            }
        }));
        rx
    }
}

#[cfg(windows)]
impl KeyBoard for SystemKeys {
    fn register(&mut self, keys: &str) -> Result<u32, String> {
        // Settings writes Win; global-hotkey calls it Super.
        let spelled = keys.split('+').map(|k| if k == "Win" { "Super" } else { k }).collect::<Vec<_>>().join("+");
        let hotkey: global_hotkey::hotkey::HotKey = spelled.parse().map_err(|e| format!("{e}"))?;
        self.manager.register(hotkey).map_err(|e| e.to_string())?;
        self.keys.insert(hotkey.id(), hotkey);
        Ok(hotkey.id())
    }

    fn unregister(&mut self, id: u32) {
        if let Some(hotkey) = self.keys.remove(&id) {
            let _ = self.manager.unregister(hotkey);
        }
    }
}
