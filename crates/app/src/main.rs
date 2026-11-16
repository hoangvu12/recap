//! Recap. `recap [--inspect]`: `--inspect` serves devtools (`zgx inspect`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    cell::RefCell,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use recap::{
    fake, fonts,
    host::{self, Host},
    windows::{self, Quit},
};
use zgx_desktop::Application;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devtools = std::env::args().any(|a| a == "--inspect") || zgx_script::inspect::asked();
    // The engine's tasks and audio run here, off the UI thread; their
    // events reach it over channels.
    let engine = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let script = host::script_path();
    if !script.is_file() {
        return Err(format!("{}: no such script (set RECAP_UI)", script.display()).into());
    }
    zgx_script::compile_ahead(&script, &[]);

    let mut app = Application::new().window(windows::root()).desktop_id("recap");
    for font in fonts::bundled() {
        app = app.font(font?);
    }
    app.run(move |cx| {
        if devtools {
            zgx_script::inspect::prepare();
        }
        let host = Host::new(&cx.ui.runtime, script.clone());
        if let Err(error) = host.script.reload() {
            eprintln!("recap: {error}");
        }
        cx.tasks.spawn(host.script.watch(Duration::from_millis(250)));
        let follow = windows::follow(&host.script, cx, Quit::WhenNoneOpen, devtools);
        let handle = cx.render(host.script.view());
        let server = devtools
            .then(|| {
                let session =
                    zgx_script::inspect::Session::new(&cx.ui, handle, &host.script, Some(cx.inspector.clone()));
                let tasks = cx.tasks.clone();
                zgx_script::inspect::serve(session, &script.display().to_string(), move |task| tasks.spawn(task))
                    .inspect(|server| eprintln!("recap: devtools at {}", server.path().display()))
                    .map_err(|error| eprintln!("recap: devtools: {error}"))
                    .ok()
            })
            .flatten();

        // Until audio capture lands: the test meeting, live.
        let started_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let (events, control) = fake::start(engine.handle());
        cx.tasks.spawn(host.follow_meeting(started_ms, events, control));
        host.open_window("notes");
        host.open_window("overlay");

        let saving = host.script.clone();
        let keep = RefCell::new(Some((follow, server, host, engine)));
        cx.on_closed(move || {
            saving.save();
            if let Some((follow, server, host, engine)) = keep.borrow_mut().take() {
                drop((follow, server, host));
                engine.shutdown_background();
            }
        });
    })
}
