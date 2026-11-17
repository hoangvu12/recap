# Recap

Live captions for the other side of your call, then a clean transcript with speaker names, a summary, next steps and decisions. Windows, Rust + zgx, Raycast look.

Start with `CLAUDE.md` and `docs/design/DESIGN.md`.

## Install

Run `recap_<version>_x64-setup.exe`. It installs for you alone, with no admin prompt:

- the app goes in `%LOCALAPPDATA%\Programs\Recap` (`recap.exe` and its Luau UI in `ui\`), with a Start menu entry;
- your meetings, settings and providers stay in `%LOCALAPPDATA%\Recap`, and API keys stay in Windows Credential Manager (under "Recap").

The build is unsigned, so the first run of a downloaded copy may show SmartScreen's "Windows protected your PC". Choose More info, then Run anyway.

To uninstall, use Settings > Apps > Installed apps > Recap, or run `uninstall.exe` in the install folder. It removes the app, the Start menu entry and the autostart value. Your meetings, settings and keys stay.

## Use

1. Open Recap from the Start menu. Home opens and the tray icon appears. Closing windows leaves Recap in the tray; quit it from the tray menu.
2. Open Settings with Ctrl+, from Home. Under Providers, add your ElevenLabs key (speech) and an AI provider for the summary (for example Claude Code, or an API key), then press Test on each one. In General, set your name, and turn on "Open Recap when Windows starts" if you want Recap to start with Windows.
3. Start notes with Ctrl+Alt+N, the tray's Start notes, or "Start new notes" on Home. The caption overlay shows the other side, and Notes keeps the live transcript and your notes.
4. Press Stop (Enter in Notes). Recap transcribes the recording, names speakers and writes the summary. The meeting then appears on Home.

## Build the installer

You need the MSVC toolchain, CMake (VS Build Tools' "C++ CMake tools", which `opus` uses), and `cargo-packager`:

```
cargo install cargo-packager --version 0.11.8 --locked
```

Then, from the repo root:

```
cargo packager --release -p recap
```

This runs `cargo build --release -p recap` first, then writes the setup to `<target>\release\recap_<version>_x64-setup.exe`. On the first run, cargo-packager downloads NSIS. The setup and the uninstaller close any running `recap.exe` first (any process with that name, a dev build too). The packaging config is `[package.metadata.packager]` in `crates/app/Cargo.toml`. The icon (`crates/app/assets/icon/`) comes from `python tools/gen_icon.py`, and `build.rs` embeds it in the exe.

To install silently, run the setup with `/S`, and add `/R` to start Recap afterwards. To uninstall silently, run `"%LOCALAPPDATA%\Programs\Recap\uninstall.exe" /S`.

## Soak test (long-run leak check)

```
recap --soak <minutes>
```

This runs the whole app (Home, Notes and the overlay) through a single endless meeting. `testdata/meeting_tts.wav` plays in a loop in real time, the captions are scripted (no API is called), and the recording is written to disk. Every minute the run writes the process's private bytes, handle count and GDI/USER objects to `soak.csv`. When the time is up, it stops the meeting, writes `soak_result.txt` (PASS or FAIL, with each measure's trend per hour) and quits. It exits with code 1 if anything kept growing. Growth counts as sustained when, after a 5-minute warm-up, the trend stays over the limit across the whole run and also across its second half. The limits are 8 MiB/h of private bytes, 60 handles/h, and 20 GDI or USER objects/h.

The soak run keeps its data in `RECAP_DATA`, or `%TEMP%\recap-soak` when that isn't set. It runs as its own instance, so an installed Recap can keep running. It doesn't touch the Run key and doesn't register the global hotkeys, but it does show its own tray icon: don't quit it from there. `soak_result.txt` is rewritten every minute (`RUNNING minute N of M: ...`), so a run that ends early still leaves its numbers. Add `--background` to keep Home closed. While a soak runs, a second `--soak` hands off to it and exits at once.

The full check is a manual step before a release. Run it for 3 hours on a release build, at low priority:

```
set CARGO_TARGET_DIR=D:\recap-impl\target
cargo build --release -p recap
start "" /low /b %CARGO_TARGET_DIR%\release\recap.exe --soak 180
```

Then read `%TEMP%\recap-soak\soak_result.txt`.
