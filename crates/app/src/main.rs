//! Recap. `recap [--inspect] [--seed] [--settings] [--replay <file.wav>] [--scripted]`
//!
//! - `--inspect` serves devtools (`zgx inspect`).
//! - `--seed` stores the test meeting first (once), so Home has one.
//! - `--settings` opens Settings too.
//! - `--replay <file.wav>` / `--scripted`: dev meetings, started at once
//!   (`recap::live` says what they replace).
//!
//! Recap's home is `RECAP_DATA` if set, else `%LOCALAPPDATA%\Recap`: it
//! holds `settings.json` and `providers.json` (keys are in Credential
//! Manager). Meetings live in Settings' storage folder, else the home.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use recap::{
    fonts,
    host::{self, Host},
    live::StartOptions,
    overlay::OverlayHost,
    processing::{Processing, ProcessingConfig},
    seed,
    session::{self, SessionConfig},
    settings::ProvidersHost,
    user_settings::{Microphone, PickFolder, SettingsHost},
    windows::{self, Quit},
};
use recap_providers::{Keyring, Providers, Registry};
use recap_storage::{Settings, Store};
use zgx_desktop::{Application, FileDialogOptions};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The store in `root`, or none (Home stays empty) when it can't open.
fn open_store(root: &Path, seed_it: bool) -> Option<Store> {
    let store = Store::open(root).map_err(|error| eprintln!("recap: {}: {error}", root.display())).ok()?;
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
    let start_options = StartOptions::from_args(std::env::args().skip(1));
    let home = std::env::var_os("RECAP_DATA")
        .map(PathBuf::from)
        .or_else(Store::default_root)
        .ok_or("no LOCALAPPDATA: set RECAP_DATA")?;
    let settings_path = home.join("settings.json");
    let settings = Settings::load(&settings_path).unwrap_or_default();
    let store = open_store(&settings.meetings_root(&home), std::env::args().any(|a| a == "--seed"));
    let open_settings = std::env::args().any(|a| a == "--settings");
    let providers =
        Providers::open(Registry::builtin(), Some(home.join("providers.json")), std::sync::Arc::new(Keyring::new()));
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
        let providers_host = ProvidersHost::register(&host.script, providers.clone(), engine.handle().clone());
        let dialogs = cx.dialogs.clone();
        let pick: PickFolder = Rc::new(move || {
            let dialogs = dialogs.clone();
            Box::pin(async move {
                dialogs
                    .open_folder(FileDialogOptions::new("Storage folder"))
                    .await
                    .map_err(|error| eprintln!("recap: folder dialog: {error}"))
                    .ok()
                    .flatten()
            })
        });
        let settings_host = SettingsHost::register(&host.script, settings_path.clone(), pick);
        settings_host.list_microphones_with(&host.script, || {
            recap_engine::microphones().into_iter().map(|(id, name)| Microphone { id, name }).collect()
        });
        settings_host.connect_store(&host, home.clone(), now_ms);
        // The overlay's window: capture exclusion, tool window, click-through
        // (#15's hotkey: `overlay.toggle_click_through()`), pin, engine switch.
        let overlay = OverlayHost::register(&host, &settings_host, providers.clone());
        // After Stop (and Recover, Retry): the after-Stop processing.
        let process: host::Process = {
            let (runtime, providers, options, settings) =
                (engine.handle().clone(), providers.clone(), start_options.clone(), settings_host.clone());
            Rc::new(move |store, id| {
                let _inside = runtime.enter();
                let settings = settings.current();
                let config = ProcessingConfig { keyterms: settings.keyterms(), my_name: settings.my_name.clone() };
                let batch = options.batch(&providers, |notice| eprintln!("recap: {notice}"));
                Processing::standard(batch).start(store, id, config)
            })
        };
        let spawn: host::Spawn = {
            let tasks = cx.tasks.clone();
            Rc::new(move |task| tasks.spawn(task))
        };
        host.set_processor(process, spawn);
        // Start new notes: capture (or a dev replay), live captions, saved.
        let starter = {
            let (host, tasks, runtime) = (host.clone(), cx.tasks.clone(), engine.handle().clone());
            let (providers, options, settings) = (providers.clone(), start_options.clone(), settings_host.clone());
            move |_| {
                if !host.is_live() {
                    let settings = settings.current();
                    let config = SessionConfig {
                        started_ms: now_ms(),
                        caption_me: settings.caption_my_speech,
                        live: settings.live_options(),
                        live_choices: StartOptions::live_choices(&providers),
                    };
                    match options.source() {
                        Ok(source) => {
                            let _inside = runtime.enter();
                            let (notify, notices) = session::notices();
                            let live = options.live(&providers, notify);
                            let started = session::start(host.store(), config.clone(), source, live, Some(notices));
                            tasks.spawn(host.follow_session(&config, started));
                            host.open_window("overlay");
                        }
                        Err(error) => host.notice(&format!("Can't start: {error}")),
                    }
                }
                host.open_window("notes");
            }
        };
        host.script.action("start_notes", starter.clone());
        if let Err(error) = host.script.reload() {
            eprintln!("recap: {error}");
        }
        cx.tasks.spawn(host.script.watch(Duration::from_millis(250)));
        let opened: windows::Opened = {
            let overlay = overlay.clone();
            Rc::new(move |id, title, wcx| {
                if id == "overlay" {
                    overlay.attach(title, wcx);
                }
            })
        };
        let follow = windows::follow(&host.script, cx, Quit::WhenNoneOpen, devtools, opened);
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
        // A dev replay starts at once.
        if start_options != StartOptions::default() {
            starter(zgx_script::Value::Nil);
        }

        let saving = host.script.clone();
        let keep = RefCell::new(Some((follow, server, host, providers_host, settings_host, overlay, engine)));
        cx.on_closed(move || {
            saving.save();
            if let Some((follow, server, host, providers_host, settings_host, overlay, engine)) =
                keep.borrow_mut().take()
            {
                drop((follow, server, host, providers_host, settings_host, overlay));
                engine.shutdown_background();
            }
        });
    })
}
