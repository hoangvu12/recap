//! The script host: what Rust gives the Luau UI (`ui/host.luau` is the other
//! side, and holds the shapes).
//!
//! ```text
//! shared   host.connected   true: the UI talks to this host, not its mocks
//!          meeting          { state, elapsed_ms, title, started_ms, id, notice }
//!          transcript       { count, open = { ["7"] = Row }, revised? = { ["2"] = Row } }
//!          captions         { { source, final, partial } }, oldest first
//!          meetings         { { id, title, started_ms, duration_ms, status } }, newest first
//!          window.<id>      whether window <id> is open
//! function transcript_row(i)    Row i (from 1), or nil
//!          meeting_detail(id)   { id, title, overview, started_ms, duration_ms, status, speakers }
//! action   stop, pause, resume  the running meeting
//!          open_meeting(id)     a stored meeting in the Notes window, read mode
//!          start_notes          a new meeting (main.rs registers it)
//! ```

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use recap_engine::{EngineEvent, MeetingControl};
use recap_storage::{MeetingId, Store};
use zgx_script::{ScriptHost, Shared, Value};

use crate::{
    home,
    presenter::{Changed, Presenter},
    session::{LiveSession, SessionConfig, SessionEvent},
};

/// Where the UI's scripts are: `RECAP_UI` (a folder with app.luau), else the
/// source tree's `ui/` (development; packaging bundles them in a later
/// ticket).
pub fn script_path() -> PathBuf {
    std::env::var_os("RECAP_UI")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("ui"))
        .join("app.luau")
}

#[derive(Clone)]
pub struct Host {
    pub script: ScriptHost,
    meeting: Shared,
    transcript: Shared,
    captions: Shared,
    meetings: Shared,
    presenter: Rc<RefCell<Presenter>>,
    control: Rc<RefCell<Option<MeetingControl>>>,
    /// The stored meeting live now, until it is saved.
    live: Rc<RefCell<Option<MeetingId>>>,
    /// Changes when Settings moves the storage folder.
    store: Rc<RefCell<Option<Store>>>,
}

fn value(json: serde_json::Value) -> Value {
    Value::from_json(&json.to_string()).unwrap_or(Value::Nil)
}

fn meeting_id(arg: &Value) -> Option<MeetingId> {
    match arg {
        Value::Str(id) if !id.is_empty() => Some(MeetingId::from(id.to_string())),
        _ => None,
    }
}

impl Host {
    /// The host of the scripts at `script`, before they load; meetings come
    /// from `store` (none: Home is empty).
    pub fn new(runtime: &zgx::reactive::Runtime, script: PathBuf, store: Option<Store>) -> Self {
        let script = ScriptHost::new(runtime, script).app_id("recap");
        script.shared("host.connected", true);
        let presenter = Presenter::new(0);
        let host = Self {
            meeting: script.shared("meeting", value(presenter.meeting())),
            transcript: script.shared("transcript", value(presenter.transcript())),
            captions: script.shared("captions", value(presenter.captions())),
            meetings: script.shared("meetings", Value::from_json("[]").unwrap_or(Value::Nil)),
            presenter: Rc::new(RefCell::new(presenter)),
            control: Rc::default(),
            live: Rc::default(),
            store: Rc::new(RefCell::new(store)),
            script,
        };
        let presenter = host.presenter.clone();
        host.script.function("transcript_row", move |arg| match arg {
            Value::Num(i) if i >= 1. => value(presenter.borrow().row(i as usize)),
            _ => Value::Nil,
        });
        for (name, act) in [
            ("stop", MeetingControl::stop as fn(&MeetingControl)),
            ("pause", MeetingControl::pause),
            ("resume", MeetingControl::resume),
        ] {
            let control = host.control.clone();
            host.script.action(name, move |_| {
                if let Some(control) = &*control.borrow() {
                    act(control);
                }
            });
        }
        let store = host.store.clone();
        host.script.function("meeting_detail", move |arg| {
            let store = store.borrow().clone();
            let (Some(store), Some(id)) = (&store, meeting_id(&arg)) else {
                return Value::Nil;
            };
            match store.meeting(&id) {
                Ok(Some(meeting)) => value(home::detail(&meeting)),
                Ok(None) => Value::Nil,
                Err(error) => {
                    eprintln!("recap: meeting {id}: {error}");
                    Value::Nil
                }
            }
        });
        let opener = host.clone();
        host.script.action("open_meeting", move |arg| {
            if let Some(id) = meeting_id(&arg) {
                opener.open_meeting(&id);
            }
        });
        host.refresh_meetings();
        host
    }

    /// Open window `id` (`ui/windows.luau` declares it).
    pub fn open_window(&self, id: &str) {
        self.script.shared(&format!("window.{id}"), false).set(Value::Bool(true));
    }

    /// Tell the user `text` in the meeting's notice line.
    pub fn notice(&self, text: &str) {
        eprintln!("recap: {text}");
        self.presenter.borrow_mut().notice(text);
        self.publish(Changed { meeting: true, ..Default::default() });
    }

    /// Whether a meeting is recording or paused now.
    pub fn is_live(&self) -> bool {
        self.control.borrow().is_some()
    }

    /// The store meetings are read from and written to now.
    pub fn store(&self) -> Option<Store> {
        self.store.borrow().clone()
    }

    /// Read and write meetings in `store` from now on (Settings moved the
    /// storage folder); Home lists its meetings.
    pub fn set_store(&self, store: Option<Store>) {
        *self.store.borrow_mut() = store;
        self.refresh_meetings();
    }

    /// Publish the stored meetings again (Home's list). The meeting live now
    /// joins it once it is saved.
    pub fn refresh_meetings(&self) {
        let Some(store) = self.store() else { return };
        match store.list() {
            Ok(mut list) => {
                if let Some(live) = &*self.live.borrow() {
                    list.retain(|m| m.id != *live);
                }
                self.meetings.set(value(home::meetings(&list)));
            }
            Err(error) => eprintln!("recap: meetings: {error}"),
        }
    }

    /// Show stored meeting `id` in the Notes window, read mode. While a
    /// meeting is live the Notes window stays on it.
    pub fn open_meeting(&self, id: &MeetingId) {
        if self.is_live() {
            eprintln!("recap: a meeting is live; {id} stays closed");
            return;
        }
        let Some(store) = self.store() else { return };
        let loaded = store.meeting(id).and_then(|m| Ok((m, store.transcript(id)?)));
        match loaded {
            Ok((Some(meeting), segments)) => {
                *self.presenter.borrow_mut() = Presenter::stored(&meeting.info, &segments);
                self.publish(Changed { meeting: true, transcript: true, captions: true });
                self.open_window("notes");
            }
            Ok((None, _)) => eprintln!("recap: no meeting {id}"),
            Err(error) => eprintln!("recap: meeting {id}: {error}"),
        }
    }

    /// Show a live session (started at `config.started_ms`): its events,
    /// until it is saved, become the UI's state; Home lists it once saved.
    /// Await what this returns on the UI thread (`cx.tasks.spawn`).
    pub fn follow_session(
        &self,
        config: &SessionConfig,
        session: LiveSession,
    ) -> impl std::future::Future<Output = ()> + 'static {
        let LiveSession { id, mut events, control } = session;
        let mut presenter = Presenter::new(config.started_ms).with_my_captions(config.caption_me);
        if let Some(id) = &id {
            presenter.saved_as(id.as_str());
        }
        *self.presenter.borrow_mut() = presenter;
        *self.control.borrow_mut() = Some(control);
        *self.live.borrow_mut() = id;
        self.publish(Changed { meeting: true, transcript: true, captions: true });
        let host = self.clone();
        async move {
            while let Some(event) = events.recv().await {
                match event {
                    SessionEvent::Engine(event) => {
                        let changed = host.presenter.borrow_mut().apply(&event);
                        host.publish(changed);
                        if matches!(event, EngineEvent::Finished(_)) {
                            host.control.borrow_mut().take();
                        }
                    }
                    SessionEvent::Notice(text) => host.notice(&text),
                    SessionEvent::Saved(info) => {
                        host.live.borrow_mut().take();
                        host.presenter.borrow_mut().saved_as(info.id.as_str());
                        host.publish(Changed { meeting: true, ..Default::default() });
                        host.refresh_meetings();
                    }
                }
            }
            // Ended without Saved (no store, or it failed): not live either.
            host.control.borrow_mut().take();
            host.live.borrow_mut().take();
        }
    }

    fn publish(&self, changed: Changed) {
        let p = self.presenter.borrow();
        if changed.meeting {
            self.meeting.set(value(p.meeting()));
        }
        if changed.transcript {
            self.transcript.set(value(p.transcript()));
        }
        if changed.captions {
            self.captions.set(value(p.captions()));
        }
    }
}
