# 0018 Per-user installer apart from the data; soak run inside the real app

## Context

Ticket #18 needs a per-user installer and a check that Recap runs for 3+ hours without leaking. cargo-packager's per-user NSIS template installs to `$LOCALAPPDATA\<product>`, which is `%LOCALAPPDATA%\Recap`, the same folder that holds our meetings and settings. The template's uninstall section can't be changed, and it doesn't know about our Run value.

## Decision

- **Installer:** we use cargo-packager 0.11.8 with its stock template, configured in `crates/app/Cargo.toml` `[package.metadata.packager]`. The template's `preinstall_section` gets `InstallDir "$LOCALAPPDATA\Programs\Recap"`, which keeps the app apart from the data, so uninstalling can never touch meetings. It also gets an `un.` section that deletes the HKCU Run value `Recap` and the toasts' AUMID key. We don't set `appdata_paths`, so uninstalling keeps the data.
- **UI:** the Luau UI ships as files in `ui\` beside `recap.exe` (`host::script_path` looks there before the source tree). Fonts are already inside the exe. `build.rs` (winresource) embeds the icon, so the windows, the taskbar and the Start menu shortcut show it. The shortcut carries the app id `app.recap.Recap`, which matches `Application::application_id`.
- **Soak:** `recap --soak <minutes>` runs the real app, not a headless test. zgx's past leaks were in the UI layer (GDI DCs), and a headless run would miss them. One endless meeting runs: a looping WAV source (`recap_engine::LoopingWav`) and the scripted provider looping with it (`ScriptedLiveStt::looping`). It runs on its own application id and data folder, never touches the Run key, and has no providers for processing. The process samples itself every minute. A measure fails when its least-squares trend after a 5-minute warm-up is over its limit both across the whole run and across its second half. So start-up that levels off passes, and steady growth fails.

## Consequences

- Hot reload still watches the installed `ui\`. Editing those files changes the installed app, which is harmless for a personal tool.
- The VC++ runtime isn't bundled: the build links it dynamically, and on a machine without it Recap won't start. This laptop has it.
- Quitting the app (the tray's Quit, and the soak's end) can fault in `D3D12Core.dll` (0xC0000005) during GPU teardown, after everything is saved. Every quit now exits at the end of `on_closed` (`std::process::exit`, after the script state is saved, the host dropped, the engine given 2 s to finish, stdout/stderr flushed), before the teardown: 0 normally, a soak's verdict for a soak. The zgx/wgpu teardown itself isn't fixed here.
- A long meeting legitimately holds its transcript in memory (about 1 MB over 3 h). The 8 MiB/h limit leaves room for that.
