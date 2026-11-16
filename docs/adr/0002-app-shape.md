# 2. The app's shape: headless engine, one script host, windows declared in Luau

Status: accepted (ticket #2)

## Decision

- **`crates/engine` (recap-engine) is headless.** A `Meeting` takes an
  `AudioSource` and a `LiveStt` (the seams; later: WASAPI capture and the
  real providers) and emits `EngineEvent`s, ending with `Finished(MeetingResult)`.
  No zgx, no storage. `ScriptedLiveStt` replays the test script, so the
  app and the tests run a real meeting without audio or keys.
- **`crates/app` (recap) is one zgx `ScriptHost`** shared by every window.
  The first zgx window is a hidden root: Recap lives in the tray, so no
  visible window is the main one. Each visible window is a `ui.window` in
  `ui/windows.luau`, open while its `window.<id>` shared state is true;
  `src/windows.rs` gives it native options by id. The app quits when the last
  declared window closes (`Quit::Resident` for the tray ticket).
- **Host state is shaped for a long transcript.** The presenter
  (`src/presenter.rs`, pure) publishes the meeting, the row count and only
  the rows still partial; a final row never changes, so the UI asks for it
  once (`transcript_row`). A two-hour meeting doesn't republish every row on
  each word.
- **Rows are placed by meeting time, not by arrival.** Each side's provider
  answers on its own, so one side's late sentence may arrive after the other
  side opened a row. It rejoins its turn when nothing of the other side was
  said in between. Rows are only ever appended, so row indexes stay stable.
- **The UI's one door to Rust is `ui/host.luau`.** Without a host (`zgx test`)
  it records invokes in `calls` and serves rows from `mock_rows`, so UI tests
  drive every screen from state they set.
