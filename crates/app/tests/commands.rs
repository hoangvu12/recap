//! What the tray, the global hotkeys and the call toasts do: one set of
//! commands, run against the real host (a live session on a fed source).

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::{executor::LocalPool, task::LocalSpawnExt};
use recap::{
    commands::{Command, Commands},
    host::{self, Host},
    session::{self, SessionConfig},
};
use recap_engine::{FrameFeed, MeetingState, Script, ScriptedLiveStt};
use zgx_script::{Value, testing::TestApp};

struct Rig {
    app: TestApp,
    host: Host,
    commands: Commands,
    started: Rc<Cell<u32>>,
    quit: Rc<Cell<bool>>,
    focused: Rc<RefCell<Vec<String>>>,
    runtime: tokio::runtime::Runtime,
    pool: LocalPool,
    feed: Option<FrameFeed>,
}

fn rig() -> Rig {
    let made: Rc<RefCell<Option<Host>>> = Rc::default();
    let out = made.clone();
    let app = TestApp::new(move |rt| {
        let host = Host::new(rt, host::script_path(), None);
        *out.borrow_mut() = Some(host.clone());
        host.script.clone()
    });
    app.frames(2);
    let host = made.borrow_mut().take().unwrap();
    let (started, quit, focused) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(false)), Rc::<RefCell<Vec<String>>>::default());
    let commands = Commands::new(
        host.clone(),
        {
            let started = started.clone();
            Rc::new(move || started.set(started.get() + 1))
        },
        {
            let focused = focused.clone();
            Rc::new(move |id: &str| focused.borrow_mut().push(id.to_owned()))
        },
        {
            let quit = quit.clone();
            Rc::new(move || quit.set(true))
        },
    );
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    Rig { app, host, commands, started, quit, focused, runtime, pool: LocalPool::new(), feed: None }
}

impl Rig {
    /// A meeting live on a source the test feeds.
    fn go_live(&mut self) {
        let _inside = self.runtime.enter();
        let (feed, source) = FrameFeed::new();
        feed.push_silence(Duration::from_millis(500));
        let config = SessionConfig::default();
        let live = session::start(None, config.clone(), Box::new(source), Arc::new(ScriptedLiveStt::new(Script::test_meeting())), None);
        self.pool.spawner().spawn_local(self.host.follow_session(&config, live)).unwrap();
        self.feed = Some(feed);
        assert!(self.until(|r| r.host.state() == Some(MeetingState::Recording)), "recording");
    }

    /// Run the host's tasks until `done` (5 s at most).
    fn until(&mut self, mut done: impl FnMut(&mut Self) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            self.pool.run_until_stalled();
            self.app.frames(1);
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn shared(&self, key: &str) -> Value {
        self.host.script.shared(key, Value::Nil).get()
    }
}

#[test]
fn hotkeys_name_their_commands() {
    assert_eq!(Command::for_hotkey("start_notes"), Some(Command::ToggleNotes));
    assert_eq!(Command::for_hotkey("pause"), Some(Command::PauseResume));
    assert_eq!(Command::for_hotkey("click_through"), Some(Command::ToggleClickThrough));
    assert_eq!(Command::for_hotkey("captions"), Some(Command::ToggleCaptions));
    assert_eq!(Command::for_hotkey("nope"), None);
}

#[test]
fn the_notes_hotkey_starts_when_idle_and_stops_when_live() {
    let mut r = rig();
    r.commands.run(Command::ToggleNotes);
    assert_eq!(r.started.get(), 1);

    r.go_live();
    r.commands.run(Command::ToggleNotes);
    assert_eq!(r.started.get(), 1, "no second meeting");
    assert!(r.until(|r| !r.host.is_live()), "stopped");
    assert_eq!(r.host.state(), Some(MeetingState::Stopped));
}

#[test]
fn pause_pauses_and_resumes_the_live_meeting() {
    let mut r = rig();
    r.commands.run(Command::PauseResume); // idle: nothing to pause
    r.go_live();
    r.commands.run(Command::PauseResume);
    assert!(r.until(|r| r.host.state() == Some(MeetingState::Paused)));
    r.commands.run(Command::PauseResume);
    assert!(r.until(|r| r.host.state() == Some(MeetingState::Recording)));
    drop(r.feed.take());
}

#[test]
fn captions_show_and_hide_the_overlay() {
    let r = rig();
    assert_eq!(r.shared("window.overlay"), Value::Bool(false));
    r.commands.run(Command::ToggleCaptions);
    assert_eq!(r.shared("window.overlay"), Value::Bool(true));
    r.commands.run(Command::ToggleCaptions);
    assert_eq!(r.shared("window.overlay"), Value::Bool(false));
}

#[test]
fn click_through_flips_the_overlay_it_is_given() {
    let r = rig();
    r.commands.run(Command::ToggleClickThrough); // none given yet: nothing
    let flips = Rc::new(Cell::new(0));
    let counted = flips.clone();
    let commands = r.commands.clone().with_click_through(Rc::new(move || counted.set(counted.get() + 1)));
    commands.run(Command::ToggleClickThrough);
    commands.run(Command::ToggleClickThrough);
    assert_eq!(flips.get(), 2);
}

#[test]
fn home_and_settings_open_and_come_forward() {
    let r = rig();
    r.commands.run(Command::OpenHome);
    assert_eq!(r.shared("window.home"), Value::Bool(true));
    r.commands.run(Command::OpenSettings);
    assert_eq!(r.shared("window.settings"), Value::Bool(true));
    assert_eq!(*r.focused.borrow(), ["home", "settings"]);
}

#[test]
fn quit_while_live_stops_the_meeting_first() {
    let mut r = rig();
    r.go_live();
    r.commands.run(Command::Quit);
    assert!(!r.quit.get(), "not before the meeting is stopped");
    assert!(r.until(|r| r.quit.get()), "quits once it has ended");
    assert_eq!(r.host.state(), Some(MeetingState::Stopped));

    let r = rig();
    r.commands.run(Command::Quit);
    assert!(r.quit.get(), "idle: at once");
}
