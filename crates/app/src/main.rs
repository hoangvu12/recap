//! Recap. `recap [--background] [--inspect] [--seed] [--settings] [--replay <file.wav>] [--scripted]`
//!
//! - `--background` starts in the tray, no window (autostart uses it).
//! - `--inspect` serves devtools (`zgx inspect`).
//! - `--seed` stores the test meeting first (once), so Home has one.
//! - `--settings` opens Settings too.
//! - `--replay <file.wav>` / `--scripted`: dev meetings, started at once
//!   (`recap::live` says what they replace).
//! - `--soak <minutes>`: the leak check, one endless scripted meeting with
//!   memory and handles sampled each minute (`recap::soak`).
//!
//! Recap lives in the tray: closing its windows leaves it running, Quit in
//! the tray menu ends it. Only one runs: a second launch brings Home of the
//! first forward and exits (zgx's application id, ADR 0011).
//!
//! Recap's home is `RECAP_DATA` if set, else `%LOCALAPPDATA%\Recap`: it
//! holds `settings.json` and `providers.json` (keys are in Credential
//! Manager). Meetings live in Settings' storage folder, else the home.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use recap::{
    calls::CallToasts,
    commands::{Command, Commands},
    fonts,
    host::{self, Host},
    live::{StartOptions, capture_options},
    overlay::OverlayHost,
    playback::{Output, Player, RodioOutput},
    processing::{Processing, ProcessingConfig},
    seed,
    session::{self, SessionConfig},
    settings::ProvidersHost,
    soak::Soak,
    user_settings::{Microphone, PickFolder, SettingsHost},
    windows::{self, OpenWindows, Quit},
};
use recap_engine::{ProviderError, ProviderErrorKind, detect::CallHint};
use recap_providers::{Keyring, Providers, Registry};
use recap_storage::{Settings, Store};
use zgx_desktop::{Application, ApplicationEvent, FileDialogOptions};

/// Who Recap is to Windows: one instance per id, and its toasts' sender.
const APPLICATION_ID: &str = "app.recap.Recap";
/// The soak run's: it runs beside the user's Recap.
const SOAK_APPLICATION_ID: &str = "app.recap.Recap.Soak";

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
    let flag = |name: &str| std::env::args().any(|a| a == name);
    let devtools = flag("--inspect") || zgx_script::inspect::asked();
    let background = flag("--background");
    // `--soak <minutes>`: the long-run leak check, an instance of its own
    // on its own data (recap::soak).
    let soak = Soak::from_args(std::env::args().skip(1));
    let soak_failed = Arc::new(AtomicBool::new(false));
    let start_options = soak.as_ref().map_or_else(|| StartOptions::from_args(std::env::args().skip(1)), Soak::start_options);
    let home = match soak {
        Some(_) => Some(Soak::home()),
        None => std::env::var_os("RECAP_DATA").map(PathBuf::from).or_else(Store::default_root),
    }
    .ok_or("no LOCALAPPDATA: set RECAP_DATA")?;
    let settings_path = home.join("settings.json");
    let settings = Settings::load(&settings_path).unwrap_or_default();
    let store = open_store(&settings.meetings_root(&home), flag("--seed"));
    let open_settings = flag("--settings");
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

    // A second launch lands here, in the first: Home comes forward.
    let reopen: Rc<RefCell<Option<Commands>>> = Rc::default();
    let on_reopen = reopen.clone();
    let mut app = Application::new()
        .window(windows::root())
        .desktop_id("recap")
        .application_id(if soak.is_some() { SOAK_APPLICATION_ID } else { APPLICATION_ID })?
        .quit_on_last_window_close(false)
        .on_application_event(move |event, _| {
            if matches!(event, ApplicationEvent::Reopen | ApplicationEvent::OpenUrls(_))
                && let Some(commands) = &*on_reopen.borrow()
            {
                commands.run(Command::OpenHome);
            }
        });
    for font in fonts::bundled() {
        app = app.font(font?);
    }
    let failed = soak_failed.clone();
    let ran = app.run(move |cx| {
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
            let soaking = soak.is_some();
            Rc::new(move |store, id| {
                let _inside = runtime.enter();
                let settings = settings.current();
                let config = ProcessingConfig { keyterms: settings.keyterms(), my_name: settings.my_name.clone() };
                if soaking {
                    // No real API in a soak run: its meeting stays unprocessed.
                    let off = || ProviderError::new(ProviderErrorKind::Auth, "no providers in a soak run");
                    return Processing::full(Err(off()), Err(off())).start(store, id, config);
                }
                let batch = options.batch(&providers, |notice| eprintln!("recap: {notice}"));
                let llm = options.llm(&providers, |notice| eprintln!("recap: {notice}"));
                Processing::full(batch, llm).start(store, id, config)
            })
        };
        let spawn: host::Spawn = {
            let tasks = cx.tasks.clone();
            Rc::new(move |task| tasks.spawn(task))
        };
        host.set_processor(process, spawn.clone());
        // Lines and voices of a stored meeting, played on their own thread.
        let (player, played) = Player::spawn(|| Box::new(RodioOutput::default()) as Box<dyn Output>);
        host.set_player(player, played, &spawn);
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
                    match options.source(&capture_options(&settings)) {
                        Ok((source, note)) => {
                            let _inside = runtime.enter();
                            let (notify, notices) = session::notices();
                            let live = options.live(&providers, notify);
                            let started = session::start(host.store(), config.clone(), source, live, Some(notices));
                            tasks.spawn(host.follow_session(&config, started));
                            if let Some(note) = note {
                                host.notice(&note);
                            }
                            host.open_window("overlay");
                        }
                        Err(error) => host.notice(&format!("Can't start: {error}")),
                    }
                }
                host.open_window("notes");
            }
        };
        host.script.action("start_notes", starter.clone());

        // The tray, the hotkeys and the call toasts run the same commands.
        let open_windows = OpenWindows::default();
        let commands = {
            let (start, focus, factory) = (starter.clone(), open_windows.clone(), cx.windows.clone());
            let overlay = overlay.clone();
            Commands::new(
                host.clone(),
                Rc::new(move || start(zgx_script::Value::Nil)),
                Rc::new(move |id: &str| focus.focus(id)),
                Rc::new(move || factory.quit()),
            )
            .with_click_through(Rc::new(move || overlay.toggle_click_through()))
        };
        *reopen.borrow_mut() = Some(commands.clone());
        // A soak run leaves the global hotkeys to the real Recap.
        let resident = resident(cx, &host, &settings_host, &commands, soak.is_none());
        match &soak {
            // The Run key is the real Recap's: a soak run leaves it alone.
            None => recap::autostart::connect(
                &settings_host,
                recap::autostart::RunKey::recap(),
                std::env::current_exe().map(|exe| recap::autostart::command_line(&exe)).unwrap_or_default(),
            ),
            // When the time is up: Quit (stops the meeting, saves, quits).
            Some(soak) => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let failed = soak_failed.clone();
                let watching = soak.watch(&home, move |verdict| {
                    failed.store(!verdict.ok, Ordering::Relaxed);
                    let _ = tx.send(());
                });
                match watching {
                    Ok(()) => {
                        let commands = commands.clone();
                        cx.tasks.spawn(async move {
                            if rx.await.is_ok() {
                                commands.run(Command::Quit);
                            }
                        });
                    }
                    Err(error) => eprintln!("recap: soak: {error}"),
                }
            }
        }

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
        let follow = windows::follow_into(&host.script, cx, Quit::Resident, devtools, open_windows, opened);
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

        if !background {
            host.open_window("home");
        }
        if open_settings {
            host.open_window("settings");
        }
        // A dev replay starts at once.
        if start_options != StartOptions::default() {
            starter(zgx_script::Value::Nil);
        }

        let saving = host.script.clone();
        let keep =
            RefCell::new(Some((follow, server, host, providers_host, settings_host, resident, overlay, engine)));
        let reopen = reopen.clone();
        let soak_exit = soak.is_some().then(|| soak_failed.clone());
        cx.on_closed(move || {
            saving.save();
            reopen.borrow_mut().take();
            if let Some((follow, server, host, providers_host, settings_host, resident, overlay, engine)) =
                keep.borrow_mut().take()
            {
                drop((follow, server, host, providers_host, settings_host, resident, overlay));
                engine.shutdown_background();
            }
            // A soak run ends here, saved, with its verdict as the exit code:
            // the GPU teardown after this can fault in D3D12Core.dll on
            // Quit (0xC0000005), which would hide the verdict.
            if let Some(failed) = &soak_exit {
                std::process::exit(i32::from(failed.load(Ordering::Relaxed)));
            }
        });
    });
    if failed.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
    ran
}

/// What keeps Recap in the tray: the icon, the hotkeys and the call watch,
/// alive while this is kept.
#[cfg(windows)]
fn resident(
    cx: &zgx_desktop::WindowContext,
    host: &Host,
    settings: &SettingsHost,
    commands: &Commands,
    with_hotkeys: bool,
) -> Box<dyn std::any::Any> {
    use recap::{hotkeys::{HotkeysHost, SystemKeys}, tray::Tray};

    let mut keep: Vec<Box<dyn std::any::Any>> = Vec::new();

    // The tray: its commands, and the icon showing whether notes are live.
    match Tray::new() {
        Ok((tray, mut chosen)) => {
            let tray = Rc::new(RefCell::new(tray));
            let meeting = host.script.shared("meeting", zgx_script::Value::Nil);
            let shown = tray.clone();
            keep.push(Box::new(cx.ui.runtime.effect(move || {
                let live = meeting.with(|m| {
                    matches!(m.field("state"), Some(zgx_script::Value::Str(s)) if &**s == "recording" || &**s == "paused")
                });
                shown.borrow_mut().set_live(live);
            })));
            keep.push(Box::new(tray));
            let commands = commands.clone();
            cx.tasks.spawn(async move {
                while let Some(command) = chosen.recv().await {
                    commands.run(command);
                }
            });
        }
        Err(error) => eprintln!("recap: tray: {error}"),
    }

    // Global hotkeys, as Settings has them.
    match SystemKeys::new() {
        Ok(_) if !with_hotkeys => {}
        Ok(keys) => {
            let mut presses = SystemKeys::presses();
            let hotkeys = HotkeysHost::connect(settings, &host.script, &cx.ui.runtime, Box::new(keys));
            let (h, commands) = (hotkeys.clone(), commands.clone());
            cx.tasks.spawn(async move {
                while let Some(id) = presses.recv().await {
                    if let Some(command) = h.pressed(id) {
                        commands.run(command);
                    }
                }
            });
            keep.push(Box::new(hotkeys));
        }
        Err(error) => eprintln!("recap: hotkeys: {error}"),
    }

    // The call toast: suggests, and acts only when clicked.
    let toasts = {
        let (host, commands) = (host.clone(), commands.clone());
        CallToasts::register(&host.script.clone(), move |hint| match hint {
            CallHint::Started { .. } if !host.is_live() => commands.run(Command::ToggleNotes),
            CallHint::Released { .. } if host.is_live() => commands.run(Command::ToggleNotes),
            _ => {}
        })
    };
    let looking = Arc::new(AtomicBool::new(settings.current().suggest_on_call));
    toasts.set_enabled(settings.current().suggest_on_call);
    {
        let (toasts, looking) = (toasts.clone(), looking.clone());
        settings.on_change(move |_, new| {
            toasts.set_enabled(new.suggest_on_call);
            looking.store(new.suggest_on_call, Ordering::Relaxed);
        });
    }
    let mut users = recap_engine::detect::poll_mic_users(looking);
    let host = host.clone();
    cx.tasks.spawn(async move {
        while let Some(on_mic) = users.recv().await {
            toasts.observe(now_ms(), &on_mic, host.is_live());
        }
    });
    Box::new(keep)
}

#[cfg(not(windows))]
fn resident(_: &zgx_desktop::WindowContext, _: &Host, _: &SettingsHost, _: &Commands, _: bool) -> Box<dyn std::any::Any> {
    Box::new(())
}
