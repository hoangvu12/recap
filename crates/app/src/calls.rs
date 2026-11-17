//! The call toast (ticket #15): when a meeting app takes the mic, a toast
//! offers Start notes; when, during notes, the mic has been free for a
//! minute, one offers Stop. Clicking is the only way it acts.
//!
//! ```text
//! shared   call_toast           { seq, title, body }: a new seq is a new toast
//! action   call_toast_clicked   seq: the toast was clicked
//! ```
//!
//! What holds the mic comes from `recap_engine::detect::poll_mic_users`
//! (Windows: a thread looking every second), or from a test.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use recap_engine::detect::{CallHint, CallWatch};
use zgx_script::{ScriptHost, Shared, Value};

struct Inner {
    watch: RefCell<CallWatch>,
    toast: Shared,
    seq: Cell<u64>,
    /// The last toast and its seq: what a click means.
    shown: RefCell<Option<(u64, CallHint)>>,
    enabled: Cell<bool>,
}

#[derive(Clone)]
pub struct CallToasts(Rc<Inner>);

/// A toast's words.
pub fn words(hint: CallHint) -> (String, &'static str) {
    match hint {
        CallHint::Started { app } => (format!("{app} call detected"), "Start notes"),
        CallHint::Released { app } => (format!("{app} left the mic"), "Stop notes"),
    }
}

impl CallToasts {
    /// Toasts on `script`; `on_click` gets the hint of the toast clicked.
    pub fn register(script: &ScriptHost, on_click: impl Fn(CallHint) + 'static) -> Self {
        let toasts = Self(Rc::new(Inner {
            watch: RefCell::default(),
            toast: script
                .shared("call_toast", Value::from_json(r#"{"seq":0,"title":"","body":""}"#).unwrap_or(Value::Nil)),
            seq: Cell::new(0),
            shown: RefCell::default(),
            enabled: Cell::new(true),
        }));
        let inner = toasts.0.clone();
        script.action("call_toast_clicked", move |arg| {
            let Value::Num(seq) = arg else { return };
            let shown = *inner.shown.borrow();
            if let Some((at, hint)) = shown
                && at == seq as u64
            {
                on_click(hint);
            }
        });
        toasts
    }

    /// Whether toasts are shown (Settings: the call toast switch).
    pub fn set_enabled(&self, on: bool) {
        self.0.enabled.set(on);
    }

    pub fn enabled(&self) -> bool {
        self.0.enabled.get()
    }

    /// `users` hold the mic at `now_ms`; `live`: notes are being taken.
    pub fn observe(&self, now_ms: u64, users: &[String], live: bool) {
        let hint = self.0.watch.borrow_mut().observe(now_ms, users, live);
        let Some(hint) = hint.filter(|_| self.enabled()) else { return };
        let seq = self.0.seq.get() + 1;
        self.0.seq.set(seq);
        *self.0.shown.borrow_mut() = Some((seq, hint));
        let (title, body) = words(hint);
        let json = serde_json::json!({ "seq": seq, "title": title, "body": body });
        self.0.toast.set(Value::from_json(&json.to_string()).unwrap_or(Value::Nil));
    }
}
