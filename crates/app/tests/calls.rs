//! The call toast, through the real UI (headless, a fake desktop): a meeting
//! app taking the mic shows "Start notes"; clicking it starts them. Nothing
//! starts without the click.

use std::{cell::RefCell, rc::Rc};

use recap::{
    calls::CallToasts,
    host::{self, Host},
};
use recap_engine::detect::{CallHint, RELEASED_FOR_MS};
use zgx_script::testing::TestApp;

fn users(exes: &[&str]) -> Vec<String> {
    exes.iter().map(|e| e.to_string()).collect()
}

fn open() -> (TestApp, CallToasts, Rc<RefCell<Vec<CallHint>>>) {
    let clicked: Rc<RefCell<Vec<CallHint>>> = Rc::default();
    let made: Rc<RefCell<Option<CallToasts>>> = Rc::default();
    let (out, c) = (made.clone(), clicked.clone());
    let app = TestApp::with_desktop(move |rt| {
        let host = Host::new(rt, host::script_path(), None);
        let toasts = CallToasts::register(&host.script, move |hint| c.borrow_mut().push(hint));
        *out.borrow_mut() = Some(toasts);
        host.script.clone()
    });
    app.frames(2);
    let toasts = made.borrow_mut().take().unwrap();
    (app, toasts, clicked)
}

#[test]
fn a_call_app_on_the_mic_offers_start_notes() {
    let (mut app, toasts, clicked) = open();
    let desktop = app.desktop.clone().unwrap();
    toasts.observe(1_000, &users(&["ms-teams.exe"]), false);
    assert!(app.wait_until(|_| desktop.notifications().len() == 1));
    let toast = &desktop.notifications()[0];
    assert_eq!(toast.title, "Teams call detected");
    assert_eq!(toast.body, "Start notes");
    assert!(clicked.borrow().is_empty(), "never starts on its own");

    desktop.click_notification(toast.tag.as_deref().unwrap());
    assert!(app.wait_until(|_| clicked.borrow().len() == 1));
    assert_eq!(clicked.borrow()[0], CallHint::Started { app: "Teams" });

    // The same call, still on the mic: no second toast.
    toasts.observe(2_000, &users(&["ms-teams.exe"]), false);
    app.frames(5);
    assert_eq!(desktop.notifications().len(), 1);
}

#[test]
fn a_mic_free_for_a_minute_offers_stop() {
    let (mut app, toasts, clicked) = open();
    let desktop = app.desktop.clone().unwrap();
    toasts.observe(0, &users(&["zoom.exe"]), true);
    toasts.observe(1_000, &users(&[]), true);
    toasts.observe(1_000 + RELEASED_FOR_MS, &users(&[]), true);
    assert!(app.wait_until(|_| desktop.notifications().len() == 1));
    let toast = &desktop.notifications()[0];
    assert_eq!(toast.title, "Zoom left the mic");
    assert_eq!(toast.body, "Stop notes");
    desktop.click_notification(toast.tag.as_deref().unwrap());
    assert!(app.wait_until(|_| clicked.borrow().len() == 1));
    assert_eq!(clicked.borrow()[0], CallHint::Released { app: "Zoom" });
}

#[test]
fn turned_off_in_settings_nothing_is_shown() {
    let (mut app, toasts, _) = open();
    let desktop = app.desktop.clone().unwrap();
    toasts.set_enabled(false);
    toasts.observe(1_000, &users(&["ms-teams.exe"]), false);
    app.frames(10);
    assert!(desktop.notifications().is_empty());
    let _ = &mut app;
}
