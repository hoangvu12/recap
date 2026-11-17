//! Story 31: when a meeting's summary is ready and its Notes window isn't
//! showing it, a Windows notification says so ("Summary ready", the
//! meeting's title); clicking it opens the meeting. Through the real UI
//! (headless, a fake desktop) and the real host, with processing faked.

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use futures::{executor::LocalPool, task::LocalSpawnExt};
use recap::{
    host::{self, Host, Process, Spawn},
    processing::ProcessingEvent,
};
use recap_storage::{MeetingId, Status, Store, Summary};
use zgx_script::{Value, testing::TestApp};

struct Rig {
    app: TestApp,
    host: Host,
    store: Store,
    pool: Rc<RefCell<LocalPool>>,
    _dir: tempfile::TempDir,
}

/// The host on a store, its processing writing a summary at once (or not).
fn rig(writes_summary: bool) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let made: Rc<RefCell<Option<Host>>> = Rc::default();
    let (out, opened) = (made.clone(), store.clone());
    let app = TestApp::with_desktop(move |rt| {
        let host = Host::new(rt, host::script_path(), Some(opened.clone()));
        *out.borrow_mut() = Some(host.clone());
        host.script.clone()
    });
    app.frames(2);
    let host = made.borrow_mut().take().unwrap();
    let pool = Rc::new(RefCell::new(LocalPool::new()));
    let spawner = pool.borrow().spawner();
    let spawn: Spawn = Rc::new(move |task| spawner.spawn_local(task).unwrap());
    let process: Process = Rc::new(move |store: Store, id: MeetingId| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        if writes_summary {
            let summary = Summary { overview: "Launch on December 1.".into(), markdown: "Launch on December 1.".into(), data: serde_json::json!({}) };
            store.set_summary(&id, &summary).unwrap();
        }
        let info = store
            .update(&id, |m| {
                m.title = "Mobile app launch on Dec 1".into();
                m.status = Status::Done;
            })
            .unwrap();
        let _ = tx.send(ProcessingEvent::Finished(info));
        rx
    });
    host.set_processor(process, spawn);
    Rig { app, host, store, pool, _dir: dir }
}

impl Rig {
    fn until(&mut self, mut done: impl FnMut(&mut Self) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            self.pool.borrow_mut().run_until_stalled();
            self.app.frames(1);
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn stopped_meeting(&self) -> MeetingId {
        let info = self.store.create(1_794_830_400_000).unwrap();
        self.store.update(&info.id, |m| m.status = Status::Processing).unwrap();
        info.id
    }

    fn notifications(&self) -> usize {
        self.app.desktop.as_ref().unwrap().notifications().len()
    }
}

#[test]
fn a_summary_ready_with_notes_closed_is_notified_and_the_click_opens_it() {
    let mut r = rig(true);
    let id = r.stopped_meeting();
    assert_eq!(r.host.script.shared("window.notes", Value::Nil).get(), Value::Bool(false));
    r.host.process(&id);
    assert!(r.until(|r| r.notifications() == 1), "notified");
    let desktop = r.app.desktop.clone().unwrap();
    let toasts = desktop.notifications();
    assert_eq!(toasts[0].title, "Summary ready");
    assert_eq!(toasts[0].body, "Mobile app launch on Dec 1");
    let tag = toasts[0].tag.clone().unwrap();

    desktop.click_notification(&tag);
    assert!(r.until(|r| r.host.window_open("notes")), "the click opens the meeting");
    let meeting = r.host.script.shared("meeting", Value::Nil).get();
    assert!(matches!(meeting.field("id"), Some(Value::Str(s)) if s.to_string() == id.as_str()), "{meeting:?}");
}

#[test]
fn no_notification_while_notes_shows_the_meeting() {
    let mut r = rig(true);
    let id = r.stopped_meeting();
    // Opening it processes it, in the Notes window.
    r.host.open_meeting(&id);
    assert!(r.until(|r| !r.host.is_processing(&id)));
    r.app.frames(10);
    assert_eq!(r.notifications(), 0);
}

#[test]
fn no_notification_without_a_summary() {
    let mut r = rig(false);
    let id = r.stopped_meeting();
    r.host.process(&id);
    assert!(r.until(|r| !r.host.is_processing(&id)));
    r.app.frames(10);
    assert_eq!(r.notifications(), 0);
}
