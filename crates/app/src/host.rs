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
//!          summary          { id, overview, topics, next_steps, decisions }: the Notes
//!                           window's meeting's summary (id "": none)
//!          speakers_dialog  { id, speakers, people }: the Name speakers dialog's
//!                           meeting (`speakers::dialog`; id "": none)
//!          playback         { key, duration_ms }: what plays now (key "": nothing)
//!          window.<id>     whether window <id> is open
//! function transcript_row(i)    Row i (from 1), or nil
//!          meeting_detail(id)   { id, title, overview, started_ms, duration_ms, status, speakers }
//! action   stop, pause, resume  the running meeting
//!          open_meeting(id)     a stored meeting in the Notes window, read mode; an
//!                               unfinished one is processed again (Recover)
//!          retry_processing(id) process meeting id again from its failed step
//!          regenerate_summary(id)  name the speakers still unnamed and write the
//!                               summary again (meetings processed without them)
//!          tick_next_step { id, index, done }  next step `index` (from 1) ticked or not
//!          set_my_notes { id, text }  what the user typed (kept at Stop)
//!          name_speakers(id)    the Name speakers dialog for meeting id (the
//!                               action panel's "Name speakers" too)
//!          rename_speakers { id, names = { S2 = "David", S1 = "" } }  names kept
//!                               ("" unnames); views relabel, the summary is
//!                               written again
//!          play { id, key, from_ms | speaker }  a line from its start, or a
//!                               voice's clip; `playback.key` says it plays
//!          stop_playback
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

use recap_engine::{EngineEvent, MeetingControl};
use recap_storage::{MeetingId, Status, Store};
use tokio::sync::mpsc::UnboundedReceiver;
use zgx_script::{ScriptHost, Shared, Value};

use crate::{
    home,
    playback::{self, Player, PlayerEvent, What},
    presenter::{Changed, Presenter},
    processing::{self, ProcessingEvent, Progress, StepId},
    session::{self, LiveSession, SessionConfig, SessionEvent},
    speakers, summary,
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
    processing: Shared,
    summary: Shared,
    /// How to process a meeting (main.rs sets it; none: meetings stay unprocessed).
    processor: Rc<RefCell<Option<(Process, Spawn)>>>,
    /// The meetings being processed now.
    running: Rc<RefCell<HashSet<MeetingId>>>,
    /// `transcript.generation`: one more each time the presenter is another.
    generation: Rc<std::cell::Cell<u64>>,
    speakers_dialog: Shared,
    /// The meeting `speakers_dialog` was made for.
    dialog_of: Rc<RefCell<Option<MeetingId>>>,
    playback: Shared,
    /// Plays lines and clips (main.rs gives it; none: nothing plays).
    player: Rc<RefCell<Option<Player>>>,
    /// What plays now (`playback.key`).
    playing: Rc<RefCell<String>>,
    /// Meetings renamed while processing: their summary is written again
    /// once it ends.
    summary_again: Rc<RefCell<HashSet<MeetingId>>>,
}

fn value(json: serde_json::Value) -> Value {
    Value::from_json(&json.to_string()).unwrap_or(Value::Nil)
}

/// `playback` when nothing plays.
fn no_playback() -> serde_json::Value {
    serde_json::json!({ "key": "", "duration_ms": 0 })
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
            processing: script.shared("processing", value(no_processing())),
            summary: script.shared("summary", value(summary::no_view())),
            processor: Rc::default(),
            running: Rc::default(),
            generation: Rc::default(),
            speakers_dialog: script.shared("speakers_dialog", value(speakers::no_dialog())),
            dialog_of: Rc::default(),
            playback: script.shared("playback", value(no_playback())),
            player: Rc::default(),
            playing: Rc::default(),
            summary_again: Rc::default(),
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
        let regenerator = host.clone();
        host.script.action("regenerate_summary", move |arg| {
            if let Some(id) = meeting_id(&arg) {
                regenerator.regenerate_summary(&id);
            }
        });
        let ticker = host.clone();
        host.script.action("tick_next_step", move |arg| {
            let json: serde_json::Value = serde_json::from_str(&arg.to_json()).unwrap_or_default();
            let (Some(id), Some(index)) = (json["id"].as_str().filter(|id| !id.is_empty()), json["index"].as_u64()) else {
                return;
            };
            ticker.tick_next_step(&MeetingId::from(id), index as usize, json["done"].as_bool().unwrap_or(false));
        });
        let notes = host.clone();
        host.script.action("set_my_notes", move |arg| {
            let json: serde_json::Value = serde_json::from_str(&arg.to_json()).unwrap_or_default();
            let (Some(id), Some(text), Some(store)) = (json["id"].as_str().filter(|id| !id.is_empty()), json["text"].as_str(), notes.store())
            else {
                return;
            };
            if let Err(error) = store.set_my_notes(&MeetingId::from(id), text) {
                eprintln!("recap: my notes of {id}: {error}");
            }
        });
        let namer = host.clone();
        host.script.action("name_speakers", move |arg| {
            if let Some(id) = meeting_id(&arg).or_else(|| namer.shown()) {
                namer.name_speakers(&id);
            }
        });
        let renamer = host.clone();
        host.script.action("rename_speakers", move |arg| {
            let json: serde_json::Value = serde_json::from_str(&arg.to_json()).unwrap_or_default();
            let (Some(id), Some(names)) = (json["id"].as_str().filter(|id| !id.is_empty()), json["names"].as_object()) else {
                return;
            };
            let names: Vec<(String, Option<String>)> = names
                .iter()
                .map(|(speaker, name)| (speaker.clone(), name.as_str().map(str::to_owned)))
                .collect();
            renamer.rename_speakers(&MeetingId::from(id), &names);
        });
        let player = host.clone();
        host.script.action("play", move |arg| {
            let json: serde_json::Value = serde_json::from_str(&arg.to_json()).unwrap_or_default();
            player.play(&json);
        });
        let stopper = host.clone();
        host.script.action("stop_playback", move |_| {
            if let Some(player) = &*stopper.player.borrow() {
                player.stop();
            }
        });
        host.refresh_meetings();
        host
    }

    /// The stored meeting the Notes window shows, if it shows one.
    fn shown(&self) -> Option<MeetingId> {
        let id = self.presenter.borrow().id().to_owned();
        (!id.is_empty() && !self.is_live()).then(|| MeetingId::from(id))
    }

    /// Play lines and clips with `player`; what it says becomes `playback`
    /// (`spawn` follows it on the UI thread).
    pub fn set_player(&self, player: Player, mut events: UnboundedReceiver<PlayerEvent>, spawn: &Spawn) {
        *self.player.borrow_mut() = Some(player);
        let host = self.clone();
        spawn(Box::pin(async move {
            while let Some(event) = events.recv().await {
                match event {
                    PlayerEvent::Playing { key, duration_ms } => {
                        *host.playing.borrow_mut() = key.clone();
                        host.playback.set(value(serde_json::json!({ "key": key, "duration_ms": duration_ms })));
                    }
                    PlayerEvent::Ended { key } => {
                        if *host.playing.borrow() == key {
                            host.playing.borrow_mut().clear();
                            host.playback.set(value(no_playback()));
                        }
                    }
                    PlayerEvent::Failed { error, .. } => {
                        host.playing.borrow_mut().clear();
                        host.playback.set(value(no_playback()));
                        host.notice(&error);
                    }
                }
            }
        }));
    }

    /// Play what `arg` says: `{ id, key, from_ms }` a line from its start,
    /// `{ id, key, speaker }` a voice's clip.
    fn play(&self, arg: &serde_json::Value) {
        let (Some(id), Some(key)) = (arg["id"].as_str().filter(|id| !id.is_empty()), arg["key"].as_str()) else {
            return;
        };
        let Some(store) = self.store() else { return };
        if self.player.borrow().is_none() {
            return;
        }
        let id = MeetingId::from(id);
        let what = match arg["speaker"].as_str() {
            Some(speaker) => {
                let segments = match store.transcript(&id) {
                    Ok(segments) => segments,
                    Err(error) => return self.notice(&error.to_string()),
                };
                What::Speaker { source: playback::side_of(speaker), turns: playback::speaker_turns(&segments, speaker) }
            }
            None => What::Line { from_ms: arg["from_ms"].as_u64().unwrap_or(0) },
        };
        if let Some(player) = &*self.player.borrow() {
            player.play(key, store.folder(&id).join(processing::AUDIO), what);
        }
    }

    /// The Name speakers dialog for meeting `id`.
    pub fn name_speakers(&self, id: &MeetingId) {
        let Some(store) = self.store() else { return };
        match speakers::dialog(&store, id) {
            Ok(view) => {
                *self.dialog_of.borrow_mut() = Some(id.clone());
                self.speakers_dialog.set(value(view));
                self.open_window("speakers");
            }
            Err(error) => eprintln!("recap: name speakers of {id}: {error}"),
        }
    }

    /// The dialog's state again, if it is on meeting `id` (names changed).
    fn refresh_dialog(&self, store: &Store, id: &MeetingId) {
        let on = self.dialog_of.borrow().as_ref() == Some(id);
        if on && let Ok(view) = speakers::dialog(store, id) {
            self.speakers_dialog.set(value(view));
        }
    }

    /// Name meeting `id`'s speakers as `names` says (Save names, an inline
    /// rename). Every view relabels; a meeting with a summary has it written
    /// again with the new names (once it is done processing, if it is).
    pub fn rename_speakers(&self, id: &MeetingId, names: &[(String, Option<String>)]) {
        let Some(store) = self.store() else { return };
        let changed = match speakers::rename(&store, id, names) {
            Ok(changed) => changed,
            Err(error) => return self.notice(&format!("Can't rename: {error}")),
        };
        if changed.is_empty() {
            return;
        }
        if self.shown().as_ref() == Some(id) {
            self.show_stored(id);
        } else {
            self.refresh_dialog(&store, id);
        }
        self.refresh_meetings();
        let has_summary = store.meeting(id).ok().flatten().is_some_and(|m| m.summary.is_some());
        if has_summary {
            self.write_summary_again(id);
        }
    }

    /// Writing summary again for meeting `id`, now or once it is done
    /// processing.
    fn write_summary_again(&self, id: &MeetingId) {
        if self.is_processing(id) {
            self.summary_again.borrow_mut().insert(id.clone());
            return;
        }
        let Some(store) = self.store() else { return };
        if let Err(error) = processing::redo(&store.folder(id), &[StepId::WritingSummary]) {
            self.notice(&format!("Can't write the summary again: {error}"));
            return;
        }
        self.process(id);
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
                self.processing.set(value(if status == Status::Done && !self.is_processing(id) {
                    no_processing()
                } else {
                    Progress::read(&store.folder(id)).view(id)
                }));
                self.show_summary(&store, id);
                self.refresh_dialog(&store, id);
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

    /// Regenerate: name the speakers still unnamed and write meeting `id`'s
    /// summary again, from its transcript (meetings processed before the AI
    /// steps, or whose summary should change).
    pub fn regenerate_summary(&self, id: &MeetingId) {
        let Some(store) = self.store() else { return };
        if self.is_processing(id) {
            return;
        }
        if let Err(error) = processing::redo(&store.folder(id), &[StepId::NamingSpeakers, StepId::WritingSummary]) {
            self.notice(&format!("Can't regenerate: {error}"));
            return;
        }
        self.process(id);
    }

    /// Tick next step `index` (from 1) of meeting `id` on or off; kept at once.
    pub fn tick_next_step(&self, id: &MeetingId, index: usize, done: bool) {
        let Some(store) = self.store() else { return };
        let Some(i) = index.checked_sub(1) else { return };
        match summary::set_done(&store, id, i, done) {
            Ok(()) => {
                if self.presenter.borrow().id() == id.as_str() {
                    self.show_summary(&store, id);
                }
            }
            Err(error) => eprintln!("recap: tick: {error}"),
        }
    }

    /// Meeting `id`'s summary as the Summary tab's state.
    fn show_summary(&self, store: &Store, id: &MeetingId) {
        let view = match store.meeting(id) {
            Ok(Some(meeting)) => match meeting.summary.and_then(|s| serde_json::from_value::<summary::SummaryData>(s.data).ok()) {
                Some(data) => summary::view(id.as_str(), &data, &meeting.info.speakers),
                None => summary::no_view(),
            },
            _ => summary::no_view(),
        };
        self.summary.set(value(view));
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
            // Renamed meanwhile: the summary again, with the new names.
            if host.summary_again.borrow_mut().remove(&id) {
                host.write_summary_again(&id);
            }
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
        self.summary.set(value(summary::no_view()));
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
        }
    }

    /// Show `presenter` from now on: another transcript, whose rows the UI
    /// reads again.
    fn replace_presenter(&self, presenter: Presenter) {
        // Another meeting: what played of the last one stops.
        if presenter.id() != self.presenter.borrow().id()
            && let Some(player) = &*self.player.borrow()
        {
            player.stop();
        }
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
