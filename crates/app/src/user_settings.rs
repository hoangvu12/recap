//! What Rust gives the Settings pages but Providers (General, Captions,
//! Audio, Glossary, Shortcuts, Storage). The UI side: `ui/host.luau` and
//! `ui/screens/settings/*.luau`.
//!
//! ```text
//! shared   settings         recap_storage::Settings, as saved
//!          settings_error   { field, message }: why the last change to `field`
//!                           ("glossary", "hotkey.<id>", "storage", "save") was refused
//!          microphones      { { id, name } }: the active capture devices
//! action   set_settings     a patch: { partial_text = false, hotkeys = {..} }…
//!          add_term         a glossary term;  remove_term  one
//!          set_hotkey       { id, keys }: "Ctrl+Shift+K", or "" for none
//!          refresh_microphones
//! async    choose_storage_folder   a folder dialog; the meetings move to it
//! ```
//!
//! Later tickets read the settings with [`SettingsHost::current`], watch them
//! off the UI thread with [`SettingsHost::subscribe`] (a tokio `watch`), or
//! on it with [`SettingsHost::on_change`].

use std::{
    cell::RefCell,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
};

use recap_storage::{GlossaryError, Settings, Store, settings::hotkey};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use zgx_script::{ScriptHost, Shared, Value};

use crate::host::Host;

/// Asks the user for a folder (the native dialog in the app, a fixed
/// answer in tests); `None` when they cancel.
pub type PickFolder = Rc<dyn Fn() -> Pin<Box<dyn Future<Output = Option<PathBuf>>>>>;

/// A capture device the Audio page offers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Microphone {
    pub id: String,
    pub name: String,
}

type Listener = Rc<dyn Fn(&Settings, &Settings)>;

struct Inner {
    path: PathBuf,
    current: RefCell<Settings>,
    watch: watch::Sender<Settings>,
    view: Shared,
    error: Shared,
    microphones: Shared,
    listeners: RefCell<Vec<Listener>>,
}

/// The user's settings on the UI thread: loaded from `settings.json`,
/// changed by the pages, saved at once and passed on.
#[derive(Clone)]
pub struct SettingsHost(Rc<Inner>);

fn value(json: serde_json::Value) -> Value {
    Value::from_json(&json.to_string()).unwrap_or(Value::Nil)
}

fn text(arg: &Value) -> Option<String> {
    match arg {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// `patch`'s fields over `base`'s, objects merged a level at a time.
fn merge(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base), serde_json::Value::Object(patch)) => {
            for (key, v) in patch {
                match base.get_mut(key) {
                    Some(slot) if slot.is_object() && v.is_object() => merge(slot, v),
                    _ => {
                        base.insert(key.clone(), v.clone());
                    }
                }
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

const HOTKEY_LABELS: [(&str, &str); 4] = [
    ("start_notes", "Start notes"),
    ("pause", "Pause"),
    ("click_through", "Click-through"),
    ("captions", "Show captions"),
];

fn glossary_message(error: GlossaryError) -> &'static str {
    match error {
        GlossaryError::Empty => "",
        GlossaryError::TooLong => "Up to 20 characters",
        GlossaryError::Duplicate => "Already in the glossary",
        GlossaryError::Full => "50 terms at most",
    }
}

#[derive(Deserialize)]
struct SetHotkey {
    id: String,
    #[serde(default)]
    keys: String,
}

impl SettingsHost {
    /// The settings in `path` (the defaults if it can't be read) and their
    /// state and actions, on `script`.
    pub fn register(script: &ScriptHost, path: PathBuf, pick_folder: PickFolder) -> Self {
        let settings = Settings::load(&path).unwrap_or_else(|error| {
            eprintln!("recap: settings: {error}; using the defaults");
            Settings::default()
        });
        let host = Self(Rc::new(Inner {
            view: script.shared("settings", value(serde_json::to_value(&settings).unwrap_or_default())),
            error: script.shared("settings_error", value(serde_json::json!({ "field": "", "message": "" }))),
            microphones: script.shared("microphones", value(serde_json::json!([]))),
            watch: watch::Sender::new(settings.clone()),
            current: RefCell::new(settings),
            listeners: RefCell::default(),
            path,
        }));

        let h = host.clone();
        script.action("set_settings", move |arg| {
            let Ok(patch) = serde_json::from_str::<serde_json::Value>(&arg.to_json()) else { return };
            let mut json = serde_json::to_value(h.current()).unwrap_or_default();
            merge(&mut json, &patch);
            match serde_json::from_value::<Settings>(json) {
                Ok(next) => h.update(|s| *s = next),
                Err(error) => eprintln!("recap: set_settings: {error}"),
            }
        });

        let h = host.clone();
        script.action("add_term", move |arg| {
            let Some(term) = text(&arg) else { return };
            let mut next = h.current();
            match next.add_term(&term) {
                Ok(()) => {
                    h.clear_error("glossary");
                    h.update(|s| *s = next);
                }
                Err(error) => h.refuse("glossary", glossary_message(error)),
            }
        });

        let h = host.clone();
        script.action("remove_term", move |arg| {
            if let Some(term) = text(&arg) {
                h.update(|s| s.remove_term(&term));
            }
        });

        let h = host.clone();
        script.action("set_hotkey", move |arg| {
            let Ok(SetHotkey { id, keys }) = serde_json::from_str::<SetHotkey>(&arg.to_json()) else { return };
            let field = format!("hotkey.{id}");
            match h.check_hotkey(&id, &keys) {
                Ok(keys) => {
                    h.clear_error(&field);
                    h.update(|s| {
                        if let Some(slot) = s.hotkeys.get_mut(&id) {
                            *slot = keys;
                        }
                    });
                }
                Err(message) => h.refuse(&field, &message),
            }
        });

        let h = host.clone();
        script.async_action("choose_storage_folder", move |_| {
            let (h, pick) = (h.clone(), pick_folder.clone());
            async move {
                let Some(folder) = pick().await else { return Ok(Value::Nil) };
                // A folder the store can't open is refused before it is kept.
                if let Err(error) = Store::open(&folder) {
                    h.refuse("storage", &error.to_string());
                    return Ok(Value::Nil);
                }
                h.clear_error("storage");
                h.update(|s| s.storage_folder = folder.to_string_lossy().into_owned());
                Ok(Value::Nil)
            }
        });
        host
    }

    /// The settings now.
    pub fn current(&self) -> Settings {
        self.0.current.borrow().clone()
    }

    /// The settings as they change, for tasks off the UI thread.
    pub fn subscribe(&self) -> watch::Receiver<Settings> {
        self.0.watch.subscribe()
    }

    /// Call `listener(old, new)` on the UI thread after each change.
    pub fn on_change(&self, listener: impl Fn(&Settings, &Settings) + 'static) {
        self.0.listeners.borrow_mut().push(Rc::new(listener));
    }

    /// Change the settings: brought into range, saved, published, passed on.
    /// If they can't be saved nothing changes.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) {
        let old = self.current();
        let mut next = old.clone();
        change(&mut next);
        let next = next.normalized();
        if next == old {
            return;
        }
        if let Err(error) = next.save(&self.0.path) {
            eprintln!("recap: settings: {error}");
            self.refuse("save", &error.to_string());
            return;
        }
        *self.0.current.borrow_mut() = next.clone();
        self.0.view.set(value(serde_json::to_value(&next).unwrap_or_default()));
        self.0.watch.send_replace(next.clone());
        let listeners: Vec<Listener> = self.0.listeners.borrow().clone();
        for listener in listeners {
            listener(&old, &next);
        }
    }

    /// The Audio page's microphones from `list` now and on
    /// `refresh_microphones`.
    pub fn list_microphones_with(&self, script: &ScriptHost, list: impl Fn() -> Vec<Microphone> + 'static) {
        let shared = self.0.microphones.clone();
        let publish = move || {
            shared.set(value(serde_json::to_value(list()).unwrap_or_default()));
        };
        publish();
        script.action("refresh_microphones", move |_| publish());
    }

    /// Apply the Storage page to `host`: meetings are read from the storage
    /// folder (else `home`) and old audio is pruned, at once and as they
    /// change. `now_ms` is the clock.
    pub fn connect_store(&self, host: &Host, home: PathBuf, now_ms: impl Fn() -> u64 + 'static) {
        let prune = move |store: Option<Store>, days: u32, now: u64| {
            let Some(store) = store else { return };
            match store.prune_audio(now, days) {
                Ok(pruned) if !pruned.is_empty() => eprintln!("recap: deleted the audio of {} meetings", pruned.len()),
                Ok(_) => {}
                Err(error) => eprintln!("recap: audio retention: {error}"),
            }
        };
        prune(host.store(), self.current().keep_audio_days, now_ms());
        let (host, me) = (host.clone(), Rc::downgrade(&self.0));
        self.on_change(move |old, new| {
            let moved = old.storage_folder != new.storage_folder;
            if moved {
                let root = new.meetings_root(&home);
                match Store::open(&root) {
                    Ok(store) => host.set_store(Some(store)),
                    Err(error) => {
                        if let Some(me) = me.upgrade() {
                            SettingsHost(me).refuse("storage", &error.to_string());
                        }
                    }
                }
            }
            if moved || old.keep_audio_days != new.keep_audio_days {
                prune(host.store(), new.keep_audio_days, now_ms());
            }
        });
    }

    /// `keys` as hotkey `id` takes it, or why not.
    fn check_hotkey(&self, id: &str, keys: &str) -> Result<String, String> {
        if !HOTKEY_LABELS.iter().any(|(known, _)| *known == id) {
            return Err("No such shortcut".into());
        }
        if keys.trim().is_empty() {
            return Ok(String::new());
        }
        let keys = hotkey(keys).ok_or("Add Ctrl, Alt, Shift or Win")?;
        let current = self.current();
        for (other, value) in current.hotkeys.all() {
            if other != id && value == keys {
                let label = HOTKEY_LABELS.iter().find(|(k, _)| *k == other).map_or(other, |(_, l)| *l);
                return Err(format!("Used by {label}"));
            }
        }
        Ok(keys)
    }

    fn refuse(&self, field: &str, message: &str) {
        self.0.error.set(value(serde_json::json!({ "field": field, "message": message })));
    }

    fn clear_error(&self, field: &str) {
        self.refuse(field, "");
    }

    /// Where the settings are saved.
    pub fn path(&self) -> &Path {
        &self.0.path
    }
}
