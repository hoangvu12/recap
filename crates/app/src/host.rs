//! The script host: what Rust gives the Luau UI (`ui/host.luau` is the other
//! side, and holds the shapes).
//!
//! ```text
//! shared   host.connected   true: the UI talks to this host, not its mocks
//!          meeting          { state, elapsed_ms, title, started_ms }
//!          transcript       { count, open = { ["7"] = Row } }
//!          captions         { { source, final, partial } }, oldest first
//!          window.<id>      whether window <id> is open
//! function transcript_row(i)    Row i (from 1), or nil
//! action   stop, pause, resume  the running meeting
//! ```

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use recap_engine::{EngineEvent, MeetingControl};
use tokio::sync::mpsc::UnboundedReceiver;
use zgx_script::{ScriptHost, Shared, Value};

use crate::presenter::{Changed, Presenter};

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
    presenter: Rc<RefCell<Presenter>>,
    control: Rc<RefCell<Option<MeetingControl>>>,
}

fn value(json: serde_json::Value) -> Value {
    Value::from_json(&json.to_string()).unwrap_or(Value::Nil)
}

impl Host {
    /// The host of the scripts at `script`, before they load.
    pub fn new(runtime: &zgx::reactive::Runtime, script: PathBuf) -> Self {
        let script = ScriptHost::new(runtime, script).app_id("recap");
        script.shared("host.connected", true);
        let presenter = Presenter::new(0);
        let host = Self {
            meeting: script.shared("meeting", value(presenter.meeting())),
            transcript: script.shared("transcript", value(presenter.transcript())),
            captions: script.shared("captions", value(presenter.captions())),
            presenter: Rc::new(RefCell::new(presenter)),
            control: Rc::default(),
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
        host
    }

    /// Open window `id` (`ui/windows.luau` declares it).
    pub fn open_window(&self, id: &str) {
        self.script.shared(&format!("window.{id}"), false).set(Value::Bool(true));
    }

    /// Show a meeting that started at `started_ms`: its events, until it
    /// finishes, become the UI's state. Await what this returns on the UI
    /// thread (`cx.tasks.spawn`).
    pub fn follow_meeting(
        &self,
        started_ms: u64,
        mut events: UnboundedReceiver<EngineEvent>,
        control: MeetingControl,
    ) -> impl std::future::Future<Output = ()> + 'static {
        *self.presenter.borrow_mut() = Presenter::new(started_ms);
        *self.control.borrow_mut() = Some(control);
        self.publish(Changed { meeting: true, transcript: true, captions: true });
        let host = self.clone();
        async move {
            while let Some(event) = events.recv().await {
                let changed = host.presenter.borrow_mut().apply(&event);
                host.publish(changed);
                if matches!(event, EngineEvent::Finished(_)) {
                    host.control.borrow_mut().take();
                }
            }
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
