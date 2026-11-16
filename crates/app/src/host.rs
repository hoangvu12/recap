//! The script host: what Rust gives the Luau UI (`ui/host.luau` is the other
//! side, and holds the shapes).
//!
//! ```text
//! shared   host.connected   true: the UI talks to this host, not its mocks
//!          meeting          { state, elapsed_ms, title, started_ms, id, notice }
//!          transcript       { count, open = { ["7"] = Row }, revised? = { ["2"] = Row } }
//!          captions         { { source, final, partial } }, oldest first
//!          meetings         { { id, title, started_ms, duration_ms, status } }, newest first
//!          processing       { id, steps = { { id, label, status, took_ms, error } } }: the
//!                           Notes window's meeting's processing (no steps: none to show)
//!          window.<id>      whether window <id> is open
//! function transcript_row(i)    Row i (from 1), or nil
//!          meeting_detail(id)   { id, title, overview, started_ms, duration_ms, status, speakers }
//! action   stop, pause, resume  the running meeting
//!          open_meeting(id)     a stored meeting in the Notes window, read mode; an
//!                               unfinished one is processed again (Recover)
//!          retry_processing(id) process meeting id again from its failed step
//!          start_notes          a new meeting (main.rs registers it)
//! ```

use std::{
    cell::RefCell,
    collections::HashSet,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
};

use recap_engine::{EngineEvent, MeetingControl, MeetingState};
use recap_storage::{MeetingId, Status, Store};
use tokio::sync::mpsc::UnboundedReceiver;
use zgx_script::{ScriptHost, Shared, Value};

use crate::{
    home,
    presenter::{Changed, Presenter},
    processing::{Progress, ProcessingEvent},
    session::{self, LiveSession, SessionConfig, SessionEvent},
};

/// Starts processing meeting `id` in `store` (on the engine's runtime).
pub type Process = Rc<dyn Fn(Store, MeetingId) -> UnboundedReceiver<ProcessingEvent>>;
/// Runs a task on the UI thread (`cx.tasks.spawn`).
pub type Spawn = Rc<dyn Fn(Pin<Box<dyn Future<Output = ()>>>)>;

/// Where the UI's scripts are: `RECAP_UI` (a folder with app.luau), else the
/// source tree's `ui/` (development; packaging bundles them in a later
/// ticket).
pub fn script_path() -> PathBuf {
    std::env::var_os("RECAP_UI")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("ui"))
        .join("app.luau")
}

/// What runs once no meeting is live ([`Host::when_idle`]).
type Then = Box<dyn FnOnce()>;

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
    /// What waits for the live meeting to end (Quit while recording).
    when_idle: Rc<RefCell<Vec<Then>>>,
    processing: Shared,
    /// How to process a meeting (main.rs sets it; none: meetings stay unprocessed).
    processor: Rc<RefCell<Option<(Process, Spawn)>>>,
    /// The meetings being processed now.
    running: Rc<RefCell<HashSet<MeetingId>>>,
    /// `transcript.generation`: one more each time the presenter is another.
    generation: Rc<std::cell::Cell<u64>>,
}

fn value(json: serde_json::Value) -> Value {
    Value::from_json(&json.to_string()).unwrap_or(Value::Nil)
}

/// `processing` when there is none to show.
fn no_processing() -> serde_json::Value {
    serde_json::json!({ "id": "", "steps": [] })
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
            when_idle: Rc::default(),
            processing: script.shared("processing", value(no_processing())),
            processor: Rc::default(),
            running: Rc::default(),
            generation: Rc::default(),
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
        let retrier = host.clone();
        host.script.action("retry_processing", move |arg| {
            let id = meeting_id(&arg).or_else(|| {
                let shown = retrier.presenter.borrow().id().to_owned();
                (!shown.is_empty()).then(|| MeetingId::from(shown))
            });
            if let Some(id) = id {
                retrier.process(&id);
            }
        });
        host.refresh_meetings();
        host
    }

    /// Open window `id` (`ui/windows.luau` declares it).
    pub fn open_window(&self, id: &str) {
        self.script.shared(&format!("window.{id}"), false).set(Value::Bool(true));
    }

    /// Whether window `id` is open (or asked to be).
    pub fn window_open(&self, id: &str) -> bool {
        self.script.shared(&format!("window.{id}"), false).with_untracked(Value::truthy)
    }

    /// Open window `id` if it is closed, close it if open.
    pub fn toggle_window(&self, id: &str) {
        let open = !self.window_open(id);
        self.script.shared(&format!("window.{id}"), false).set(Value::Bool(open));
    }

    /// The meeting's state: the live one's, else the one shown (none yet).
    pub fn state(&self) -> Option<MeetingState> {
        self.presenter.borrow().state()
    }

    /// Stop the live meeting (nothing when none is).
    pub fn stop(&self) {
        if let Some(control) = &*self.control.borrow() {
            control.stop();
        }
    }

    /// Pause the live meeting, or resume it while paused.
    pub fn toggle_pause(&self) {
        let paused = self.state() == Some(MeetingState::Paused);
        if let Some(control) = &*self.control.borrow() {
            if paused { control.resume() } else { control.pause() }
        }
    }

    /// Run `then` once no meeting is live: now, or when the live one ends.
    pub fn when_idle(&self, then: impl FnOnce() + 'static) {
        if self.is_live() {
            self.when_idle.borrow_mut().push(Box::new(then));
        } else {
            then();
        }
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
    /// meeting is live the Notes window stays on it. One that never finished
    /// processing is processed again from where it stopped (Recover).
    pub fn open_meeting(&self, id: &MeetingId) {
        if self.is_live() {
            eprintln!("recap: a meeting is live; {id} stays closed");
            return;
        }
        let Some(status) = self.show_stored(id) else { return };
        self.open_window("notes");
        if status != Status::Done {
            self.process(id);
        }
    }

    /// Stored meeting `id` (its transcript, pauses and processing) as the
    /// Notes window's state; its status, or none when it can't be read.
    fn show_stored(&self, id: &MeetingId) -> Option<Status> {
        let store = self.store()?;
        let loaded = store.meeting(id).and_then(|m| Ok((m, store.transcript(id)?)));
        match loaded {
            Ok((Some(meeting), segments)) => {
                let pauses = session::read_pauses(&store.folder(id));
                self.replace_presenter(Presenter::stored_with_pauses(&meeting.info, &segments, &pauses));
                self.publish(Changed { meeting: true, transcript: true, captions: true });
                let status = meeting.info.status;
                self.processing.set(value(if status == Status::Done {
                    no_processing()
                } else {
                    Progress::read(&store.folder(id)).view(id)
                }));
                Some(status)
            }
            Ok((None, _)) => {
                eprintln!("recap: no meeting {id}");
                None
            }
            Err(error) => {
                eprintln!("recap: meeting {id}: {error}");
                None
            }
        }
    }

    /// How meetings are processed after Stop (and recovered): `process`
    /// starts it, `spawn` runs its follower on the UI thread.
    pub fn set_processor(&self, process: Process, spawn: Spawn) {
        *self.processor.borrow_mut() = Some((process, spawn));
    }

    /// Whether meeting `id` is being processed now.
    pub fn is_processing(&self, id: &MeetingId) -> bool {
        self.running.borrow().contains(id)
    }

    /// Process meeting `id` from where it is (Stop, Retry, Recover), unless
    /// it is being processed already. The Notes window shows its progress
    /// while it shows the meeting; Home lists it again once it is done.
    pub fn process(&self, id: &MeetingId) {
        let Some(store) = self.store() else { return };
        let Some((process, spawn)) = self.processor.borrow().clone() else {
            eprintln!("recap: no processing; {id} stays unprocessed");
            return;
        };
        if !self.running.borrow_mut().insert(id.clone()) {
            return;
        }
        let mut events = process(store, id.clone());
        let (host, id) = (self.clone(), id.clone());
        spawn(Box::pin(async move {
            let mut failed = false;
            while let Some(event) = events.recv().await {
                let shown = !host.is_live() && host.presenter.borrow().id() == id.as_str();
                match event {
                    ProcessingEvent::Progress(progress) => {
                        failed = progress.failed().is_some();
                        if shown {
                            host.processing.set(value(progress.view(&id)));
                        }
                    }
                    // What the step wrote (the refined transcript…) replaces
                    // what was shown.
                    ProcessingEvent::StepDone(_) => {
                        if shown {
                            host.show_stored(&id);
                        }
                        host.refresh_meetings();
                    }
                    ProcessingEvent::Finished(_) => {
                        if shown {
                            host.processing.set(value(no_processing()));
                        }
                        host.refresh_meetings();
                    }
                    ProcessingEvent::Stopped(error) => {
                        if !failed {
                            host.notice(&error);
                        }
                    }
                }
            }
            host.running.borrow_mut().remove(&id);
        }));
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
        self.replace_presenter(presenter);
        *self.control.borrow_mut() = Some(control);
        *self.live.borrow_mut() = id;
        self.processing.set(value(no_processing()));
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
                        host.control.borrow_mut().take();
                        host.presenter.borrow_mut().saved_as(info.id.as_str());
                        host.publish(Changed { meeting: true, ..Default::default() });
                        host.refresh_meetings();
                        // After Stop: the refined transcript, the names, the summary.
                        host.processing.set(value(Progress::default().view(&info.id)));
                        host.process(&info.id);
                    }
                }
            }
            // Ended without Saved (no store, or it failed): not live either.
            host.control.borrow_mut().take();
            host.live.borrow_mut().take();
            let waiting: Vec<_> = host.when_idle.borrow_mut().drain(..).collect();
            for then in waiting {
                then();
            }
        }
    }

    /// Show `presenter` from now on: another transcript, whose rows the UI
    /// reads again.
    fn replace_presenter(&self, presenter: Presenter) {
        *self.presenter.borrow_mut() = presenter;
        self.generation.set(self.generation.get() + 1);
    }

    fn publish(&self, changed: Changed) {
        let p = self.presenter.borrow();
        if changed.meeting {
            self.meeting.set(value(p.meeting()));
        }
        if changed.transcript {
            let mut transcript = p.transcript();
            transcript["generation"] = self.generation.get().into();
            self.transcript.set(value(transcript));
        }
        if changed.captions {
            self.captions.set(value(p.captions()));
        }
    }
}
