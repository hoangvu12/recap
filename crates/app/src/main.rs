//! Recap. `recap [--inspect] [--seed] [--settings]`
//!
//! - `--inspect` serves devtools (`zgx inspect`).
//! - `--seed` stores the test meeting first (once), so Home has one.
//! - `--settings` opens Settings too.
//!
//! Meetings live in `RECAP_DATA` if set, else `%LOCALAPPDATA%\Recap`; the
//! providers in `providers.json` there, their keys in Credential Manager.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    cell::RefCell,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use recap::{
    fake, fonts,
    host::{self, Host},
    seed,
    settings::ProvidersHost,
    windows::{self, Quit},
};
use recap_providers::{Keyring, Providers, Registry};
use recap_storage::Store;
use zgx_desktop::Application;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The store, or none (Home stays empty) when it can't open.
fn open_store(seed_it: bool) -> Option<Store> {
    let root = std::env::var_os("RECAP_DATA").map(PathBuf::from).or_else(Store::default_root)?;
    let store = Store::open(&root).map_err(|error| eprintln!("recap: {}: {error}", root.display())).ok()?;
    if seed_it {
        match seed::test_meeting(&store, now_ms()) {
            Ok(info) => eprintln!("recap: seeded {} in {}", info.id, store.folder(&info.id).display()),
            Err(error) => eprintln!("recap: seed: {error}"),
        }
    }
    Some(store)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devtools = std::env::args().any(|a| a == "--inspect") || zgx_script::inspect::asked();
    let store = open_store(std::env::args().any(|a| a == "--seed"));
    let open_settings = std::env::args().any(|a| a == "--settings");
    let providers = Providers::open(
        Registry::builtin(),
        store.as_ref().map(|s| s.root().join("providers.json")),
        std::sync::Arc::new(Keyring::new()),
    );
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
        let host = Host::new(&cx.ui.runtime, script.clone(), store.clone());
        // Start new notes: until audio capture lands, the test meeting, live.
        let starter = {
            let (host, tasks, runtime) = (host.clone(), cx.tasks.clone(), engine.handle().clone());
            move |_| {
                if !host.is_live() {
                    let (events, control) = fake::start(&runtime);
                    tasks.spawn(host.follow_meeting(now_ms(), events, control));
                    host.open_window("overlay");
                }
                host.open_window("notes");
            }
        };
        host.script.action("start_notes", starter);
        let providers_host = ProvidersHost::register(&host.script, providers.clone(), engine.handle().clone());
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

        host.open_window("home");
        if open_settings {
            host.open_window("settings");
        }

        let saving = host.script.clone();
        let keep = RefCell::new(Some((follow, server, host, providers_host, engine)));
        cx.on_closed(move || {
            saving.save();
            if let Some((follow, server, host, providers_host, engine)) = keep.borrow_mut().take() {
                drop((follow, server, host, providers_host));
                engine.shutdown_background();
            }
        });
    })
}
