# zgx for a Windows meeting notetaker: research

Read-only research, 2026-11-16. Path prefixes used below:

- `ZGX` = `C:\Users\HP MEDIA\.cargo\git\checkouts\zgx-b5d425ec12da2cd4\24b3587` (zgx main at commit 24b35872, 2026-11-15; workspace version 0.1.10).
- `ZERON` = `C:\Users\HP MEDIA\Desktop\nguyenvu\zeron-autonotify\apps\zgx` (the user's real zgx app: a Luau UI with a Rust host).
- `GLASS` = `C:\Users\HP MEDIA\Desktop\nguyenvu\zgx-glass` (the user's zgx fork, branch `windows/popup-glass`).

What was checked on this machine:

- I scaffolded a `zgx new demo --template list` app under `%TEMP%`, then deleted it. `zgx test` passed 3 of 3 and `zgx lint` was clean.
- `zgx check` failed: luau-lsp isn't installed.
- `zgx-run` isn't on PATH, so `zgx run` and `zgx audit` can't start yet.
- The installed `zgx.exe` was built at rev `2e2334a` (`C:\Users\HP MEDIA\.cargo\.crates.toml`) without `-DNDEBUG`, and it prints a warning that Luau asserts are on.

---

## 1. App structure

### Recommendation: Luau UI with a Rust host

Write all the logic in Rust: audio capture, the ElevenLabs websocket, HTTP and SQLite. Write the windows' content in Luau.

| Why | Source |
|---|---|
| The user's own production zgx app is built this way. Its interface is Luau in `ui/` and hot-reloads keeping state; Rust makes the windows and answers the scripts' actions. | `ZERON\README.md:3-8` |
| Luau and Rust perform about the same: streaming CPU within noise (2.6% each), about 0.9 MiB more memory for Luau, idle zero for both. | `ZGX\docs\scripting.md:668-670` |
| Only Luau gets hot reload that keeps state, a failed reload keeping the last good UI, `zgx test`, `zgx inspect`, `zgx audit` and `zgx lint`. | `ZGX\docs\scripting.md:28,463-519`; `ZGX\docs\agents\inspect.md:1-12,355` |
| Two key widgets exist only in Luau. Markdown rendering (`ui.markdown_view`) is Luau-only, implemented in `zgx-script`. The ready-made transcript list (`ui.list { measured, follow, key }`) is Luau too. | `ZGX\crates\zgx-script\src\markdown\view.rs:1-8`; `ZGX\docs\reference\scrolling.md:45` |
| Pure Rust UI is supported and type-safe: `compose`, `component`, signals, zgx-components. It loses hot reload and inspect: `zgx inspect` needs a `ScriptHost` session. | `ZGX\docs\getting-started.md:43-80`; `ZGX\docs\agents\inspect.md:355` |
| Luau limits to keep in mind: a 1 GiB heap for app scripts and a 50 ms CPU budget per call. So don't hold a 2-hour transcript as one Luau table. Keep it in Rust and serve rows on demand. | `ZGX\docs\scripting.md:28` |

### How Rust exposes functions, state and events to Luau (`ScriptHost`)

| Mechanism | Rust side | Luau side | Source |
|---|---|---|---|
| Fire-and-forget command | `host.action("stop", \|v: Value\| …)` | `ui.action("stop", arg)` / `ui.invoke` | `ZGX\crates\zgx-script\src\lib.rs:2608-2614` |
| Async command returning a result | `host.async_action("summarize", \|v\| async { Ok(Value) })` runs on the UI-thread executor (doesn't need to be `Send`) | `ui.call(name, arg)`, a promise | `lib.rs:2627-2642` |
| Sync read of data Rust keeps | `host.function("row", \|i\| Value)` (keep it quick, no I/O) | `ui.call_sync(name, arg)`, made for rows of a virtualized list | `lib.rs:2615-2626` |
| Rust-to-Luau state (view model) | `let s = host.shared("live", init); s.set(v)`. `set` reconciles, so republishing a whole model costs only what changed. `s.append(text)` appends to a string. | `ui.shared("live", nil)`, read reactively | `lib.rs:2892-2899`; `ZGX\crates\zgx-script\src\value.rs:2797-2834` |
| Data format | `Value` (Nil/Bool/Num/Str/List/Map…), plus `Value::record`, `Value::from_json` and `to_json`. Use `serde_json` to get there; Value has no serde derive. | tables | `value.rs:21-41,419,515` |
| Multiple windows from one host | `host.windows()` and `host.window_view(id)`. The Rust host opens each window itself, so it can pass any `WindowOptions` (transparent, level, glass). `zgx_run::follow_windows` automates this but uses Luau window options. | `ui.window { id, open, … }` | `lib.rs:3099-3107`; `ZGX\crates\zgx-run\src\lib.rs:597-600`; `ZGX\docs\reference\native.md:93-106` |
| Release builds | `zgx_script::build::bundle("ui","app")` plus `ScriptHost::bundled` | — | `ZGX\docs\scripting.md:521-537` |

### Pushing updates from background threads or tokio into the UI

- The UI objects (`Signal`, `Shared`) are `Rc`, UI-thread only. Use thread-safe channels and do the writes from a UI task (`ZGX\docs\application.md:199`).
- The UI executor's wakers are cross-thread: "Wakers may be used across threads" (`ZGX\crates\zgx\src\task.rs:1`).
- The desktop host wires a wake from another thread to `EventLoopProxy` (`ZGX\crates\zgx-desktop\src\application.rs:2315-2323`).
- So any runtime-agnostic async receiver awaited in `cx.tasks.spawn(async {…})` wakes the loop with no polling: `tokio::sync::mpsc`, `futures::channel::mpsc` or a oneshot.
- `zgx::task::latest::<T>()` is zgx's own cross-thread channel (`task.rs:276-313`). It **keeps only the latest value**, so it suits the live partial line or level meters but not finalized transcript segments. For those, use a lossless mpsc and batch the writes.
- The user's app uses this exact pattern: `latest()`, then `cx.tasks.spawn(async { while let Some(v) = heard.recv().await { shared.set(v) } })` (`ZERON\src\transparency.rs:33-39`).
- For blocking jobs, `zgx::background::BackgroundPool::new(workers, cap)` gives an awaitable result (`ZGX\docs\application.md:221-236`).
- `WindowHandle` is thread-safe; `wake()` and `request_redraw()` are rarely needed (`ZGX\crates\zgx-desktop\src\application.rs:342-348,509-522`).

### Using tokio alongside zgx

- zgx has no tokio dependency. Run a separate multi-thread tokio runtime on its own threads for the websocket, HTTP and SQLite work.
- Hand results back over channels.
- Don't run tokio timers or I/O directly inside zgx tasks: they need tokio's reactor. Awaiting tokio channels or JoinHandles from zgx tasks is fine.
- The user's app does this: a private `new_multi_thread().worker_threads(1)` runtime bridged back through `latest()` (`ZERON\src\about.rs:74-94`). Its websocket thread stores a `Waker` and the UI awaits it through `poll_fn` (`ZERON\src\engine\rpc.rs:56-95`).

---

## 2. Windows

| Need | Supported? | How / source |
|---|---|---|
| Multiple windows | Yes | `cx.windows.open(options, builder)`. Windows share one reactive runtime. (`ZGX\docs\application.md:238`; `ZERON\src\main.rs:427,463`) |
| Always on top | Yes | `WindowOptions::level = WindowLevel::AlwaysOnTop` (Windows "topmost"), and `WindowHandle::set_level`. (`ZGX\crates\zgx-desktop\src\window_info.rs:51-58`; `application.rs:370-374`) |
| Borderless + transparent | Yes | `transparent: true`, `decorations: false`, `shadow`. Or `titlebar: Titlebar::Hidden`, which is frameless but keeps 5-pt resize bands and Win11 corners. Use `View::window_drag()` / `window_drag = true` for move regions. (`ZGX\docs\window-controls.md:29,33`; `ZGX\docs\windows.md:448-456`) |
| Mica / Acrylic | Yes | `glass: Some(Glass::new(GlassMaterial::…))`. Hud, Menu and Popover map to Acrylic; WindowBackground and Sidebar map to Mica (Win11 22H2+). `.plain()` is an untinted acrylic blur through `SetWindowCompositionAttribute` (Win10 1809+). Change it live with `set_glass`. (`ZGX\docs\window-controls.md:29,59`; `ZGX\docs\windows.md:464-491`; `GLASS\crates\zgx-desktop\src\window_chrome.rs:388-405,547-629`) |
| Acrylic on an overlay that is never activated | Use `.plain()` | DWM draws the transient/Acrylic system backdrop flat behind a window that was never activated (`ZGX\docs\windows.md:550-552`). A caption overlay is seldom activated, so `.plain()` is the safer choice; the user's app also uses `.plain()` (`ZERON\src\main.rs:1150-1161`). |
| Click-through | Whole-window only | `WindowHandle::set_pointer_passthrough(bool)` calls winit `set_cursor_hittest(!v)` (`application.rs:364-369,4753-4755`). There is no per-region click-through. Toggle it, for example on hover of a handle or on a hotkey. The user's intro window uses it (`ZERON\src\main.rs:373,394-397`). |
| Resizable | Yes | `resizable`, `min_size` / `max_size`, `set_min_inner_size` (`application.rs:38-121,398-411`) |
| Remember position | Yes | `frame_autosave: Some("captions")` is saved to `HKCU\Software\zgx\<exe>\Frames` and restored, with a guard against an unplugged monitor (`ZGX\docs\application.md:183`) |
| Hidden at start, shown whole | Yes | `visible: false` then `set_visible(true)` (`ZGX\docs\application.md:175`) |
| Hide from screen capture (`SetWindowDisplayAffinity`) | **No** | No API anywhere in `crates\`: a search for DisplayAffinity, content_protect and skip_taskbar found nothing. You need raw Win32. |
| Hide from taskbar / tool window | **No** | `WindowOptions` has no skip-taskbar field (`application.rs:38-121`), and the winit attributes zgx sets don't include it (`window_info.rs:443-459`). You need raw Win32: add `WS_EX_TOOLWINDOW` with `SetWindowLongPtrW`. |
| Non-activating overlay | **No** | There's no window option for it. Only zgx's own popups are `WS_EX_NOACTIVATE` (`GLASS\crates\zgx-desktop\src\popup_window\win32.rs:461`). |

### Getting the HWND

- zgx exposes no HWND or `raw_window_handle` accessor.
- `WindowHandle` holds only an event-loop proxy and a token (`application.rs:344-348`); its public methods are listed at `application.rs:351-522`.
- zgx reads the handle only internally (`file_dialog.rs:381-382`, `input_pane.rs:16`).
- The user's app never touches an HWND directly; it uses `windows-sys` for other Win32 calls (`ZERON\src\main.rs:1019-1031`).

Workarounds:

- **(a)** On the UI thread, after the first frame (`cx.frames().next().await`), call `EnumThreadWindows(GetCurrentThreadId())`. Match winit's class `"Window Class"` (`ZGX\vendor\winit\src\platform_impl\windows\mod.rs:51`) plus a unique title. Then call `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` (Win10 2004+) and/or set `WS_EX_TOOLWINDOW`.
- Popup windows (menus, tooltips) are separate owned windows. Exclude them from capture too, or keep menus inside the window with `ZGX_POPUP_WINDOWS=0` (`ZGX\docs\windows.md:529-577`).
- **(b)** Upstream a small `WindowContext`/`WindowHandle` method for the native handle, or for display affinity and skip-taskbar. The repo's rule is "Fix zgx, not around it" (`ZGX\AGENTS.md:29`).

Note on capture: a covered zgx window stops drawing even while Teams or OBS captures it. `ZGX_RENDER_WHEN_COVERED=1` keeps it drawing (`ZGX\docs\windows.md:443-446`).

---

## 3. Tray, hotkey, toasts, login start, single instance

| Feature | zgx provides | Otherwise | Source |
|---|---|---|---|
| System tray icon + menu | **No** | `tray-icon` crate, created on the UI thread (winit pumps that thread's messages). Its events go through a channel to a zgx task. | A search of `ZGX\crates` for tray / `Shell_NotifyIcon` found nothing; the user's app has no tray either (Explore survey of `ZERON\Cargo.toml`) |
| Run with no window open (tray-resident) | Yes | `Application::quit_on_last_window_close(false)`, `WindowFactory::open/quit` | `ZGX\docs\platform-services.md:149`; `ZGX\crates\zgx-desktop\src\application.rs:1039` |
| Global hotkey | **No** | `global-hotkey` crate (RegisterHotKey). zgx's own shortcuts are window-scoped: menu accelerators and `ui.command { key }`. | Search found nothing; `ZGX\docs\platform-services.md:93-99`; `ZGX\docs\scripting.md:226` |
| Toast notifications | **Yes** | Rust: `Platform::notify(Notification)` plus `next_notification_click`. Luau: `ui.notify { title, body, tag, on_click }`. Needs `Application::application_id`, which registers `HKCU\Software\Classes\AppUserModelId\<id>` on first use. Taskbar badge: `ui.badge` / `Platform::set_badge`. | `ZGX\docs\platform-services.md:162-164`; `ZGX\docs\reference\native.md:265-286`; `ZGX\crates\zgx-desktop\src\notify.rs` |
| Start on login | **No** | `HKCU\...\Run` through the `auto-launch` crate or `winreg`, or the installer. zgx adds no packaging (`ZGX\docs\windows.md:731-735`). | Search found nothing |
| Single instance | **Yes** | `Application::application_id("com.x.Notes")`: a named pipe per ID and session. A second launch forwards its args (`ApplicationEvent::OpenUrls` / `Reopen`) and exits. The ID is also the AppUserModelID. | `ZGX\docs\platform-services.md:137-148`; `ZGX\docs\windows.md:723-729,788-800` |
| Native file dialogs and message boxes | Yes | `cx.dialogs`, `prompt` | `ZGX\docs\platform-services.md:1-45,127-131` |
| App icon, version info, manifest | Build script (`winresource`) | — | `ZGX\docs\windows.md:519-523,777-786` |

---

## 4. Lists and text

| Need | Luau | Rust | Source |
|---|---|---|---|
| Long virtualized transcript, auto-scroll, variable height | `ui.list { count, key, measured = true, follow = true, row = … }`. `follow` sticks to the end until the user scrolls away, and `key` keeps rows when items are inserted. | `measured_virtual_list` / `variable_virtual_list`, and `.follow_end(true)` on scroll viewports | `ZGX\docs\reference\scrolling.md:21,39-55`; `ZGX\docs\scripting.md:140,156`; `ZGX\crates\zgx\src\compose_variable.rs:157,177`; `ZGX\crates\zgx\src\compose.rs:1012-1017`; example `ZGX\crates\zgx-script\examples\long_transcript\app.luau` |
| Live-updating last line, partial text greyed | A row reads a shared state. Use `ui.rich { span{final}, span{partial, color="$text.tertiary"} }`. Only the changed row re-renders. | `rich_text_signal(\|\| Vec<TextSpan>)` | `ZGX\docs\scripting.md:142`; `ZGX\docs\rich-text.md:16-23` |
| Selectable, copyable text | `selectable = true` on a container: drag, double/triple click, Ctrl+A/C, context-menu Copy | `.selectable(true)` | `ZGX\docs\scripting.md:255`; `compose.rs:1948-1951` |
| Chat-bubble rows | ordinary `row`/`column` with `bg`, `rounded`, `max_w = "75%"`, `wrap` | same, fluent API | `ZGX\docs\scripting.md:240-262` |
| Notes editor | `ui.textarea`: undo, IME, spellcheck, `wrap`, `highlight` for Markdown-style styling, `find`. A **plain-text** editor. | `text_area(label, signal)` | `ZGX\docs\reference\controls.md:43-138`; `compose.rs:3306-3321` |
| Rich-text (WYSIWYG) editor | **No** | **No** | "display text, not a rich-text editor" (`ZGX\docs\rich-text.md:39`); "A full rich-text document editor … remain separate" (`ZGX\docs\ROADMAP.md:143`) |
| Markdown for the summary | `ui.markdown_view { text }`: incremental, streams, code blocks with Copy, tables, theme colors | No view in Rust (`zgx_script::markdown` is public) | `ZGX\docs\scripting.md:143-144`; `ZGX\crates\zgx-script\src\markdown\view.rs:1-8` |
| Inline editable title | `ui.input { value = function, on_commit }`, styled flat | `text_input` | `ZGX\docs\reference\controls.md:49-51` |
| Combo box with suggestions | `ui.combobox`: editable, virtual list, async `loading` | `zgx_components::combobox` | `ZGX\docs\reference\controls.md:204`; `ZGX\crates\zgx-components\src\combobox.rs:1-8` |
| Tabs | `ui.tabs` (WAI-ARIA, closable, reorderable) | `zgx_components::tabs` | `ZGX\crates\zgx-components\src\tabs.rs:1-7` |
| Dialogs / modals | `ui.modal { open, build }` (rebuilt on each open), or a separate `ui.window` | `modal(label, open)` | `ZGX\AGENTS.md:52`; `ZGX\docs\application.md:114-130` |
| Menus / context menus | `ui.menu`, `ui.menu_item`, `ui.submenu`, `context_menu = fn`. Popups get their own windows on Windows (Win11 acrylic). | `menu`, `menu_item`, `View::context_menu`. The app menu bar is rendered, not native Win32. | `ZGX\docs\scripting.md:157,232-234`; `ZGX\docs\windows.md:529-566,842` |
| In-app toasts | `ui.toast` | `zgx_components::toast` | `ZGX\crates\zgx-components\src\toast.rs:1-7` |
| Theming, light/dark | `"$token"` styles, `ui.theme.follow_system{dark,light}`, `ui.theme.set`, per-subtree `theme =`. Windows accent, high contrast, text size and transparency settings are followed. | `Themes::follow_system`, `WindowHandle::set_theme` | `ZGX\docs\scripting.md:539-581`; `ZGX\docs\windows.md:610-650`; `ZGX\docs\window-controls.md:59` |
| Forms, settings | `ui.form`, `ui.persist` (JSON at `%APPDATA%\<app_id>\script-state`) | — | `ZGX\docs\scripting.md:378,387-396` |

---

## 5. Audio playback

- zgx has **no audio playback API**. A search for rodio, cpal, PlaySound and sound found none in the runtime crates.
- `video.audio` exists only in zgx-video's offscreen renderer (`ZGX\docs\video.md:136-146`).
- A WebView2 `<audio>` would work, behind the `webview2` feature (`ZGX\docs\native-surfaces.md:32`), but that is heavy.
- Use **rodio** (or cpal) on its own thread and drive play/stop from a host action. Report progress through `latest()`.
- The user's app plays chimes by shelling out to `afplay`/`paplay` (`ZERON\src\settings.rs:506`). On my reading that is silent on Windows; I didn't run it.

---

## 6. Testing and devtools

| Tool | What it gives us | Source |
|---|---|---|
| Hot reload | `host.watch(250ms)` reloads on save and keeps state and focus. A failed reload keeps the last good UI and shows a banner with file:line. | `ZGX\docs\scripting.md:18-28,125` |
| `zgx test` | `*.test.luau`, headless: `mount()`, `click`, `type`, `key`, `wait`, `state`/`set_state`, a fake clipboard and dialogs, deterministic 1/60 s frames. Ran OK here. | `ZGX\docs\scripting.md:463-496` |
| `TestApp` (Rust) | Mounts a `ScriptHost` headless with mocked `async_action`s. Use it to test the Rust-to-Luau bridge (for example inject transcript events) in `cargo test`. `calls()` checks that nothing runs while idle. | `ZGX\docs\scripting.md:498-519` |
| `zgx inspect` | Drives a running app: `tree`, `node`, `state`, `errors`, `snapshot`, `click`, `type`, `key`, `frames`, `memory`, record/replay of gestures. A Rust host serves it through `zgx_script::inspect::serve(...)` (`zgx-script` `inspect` feature, as the user's app enables). | `ZGX\docs\agents\inspect.md:1-60,101,124-139,195-213,355` |
| Layout inspector | Ctrl+Alt+Shift+I, or `ZGX_INSPECT=1`. Rust apps too. | `ZGX\docs\application.md:75` |
| `zgx lint` / `zgx check` / `zgx audit` | Performance anti-patterns / luau-lsp types (not installed) / accessibility | `ZGX\docs\scripting.md:56-121` |
| Panic containment | A panic in a handler, effect or task shows a banner and the app continues | `ZGX\docs\application.md:246-258` |

How this helps us iterate:

- Build the Luau UI against fake data published via `host.shared`, with hot reload.
- Lock behavior with `zgx test` / `TestApp`.
- Use `zgx inspect` to check the transcript under replayed scrolls and memory growth over a long session.

---

## 7. Maturity risks

| Risk | Evidence |
|---|---|
| **Windows is not a primary target.** "Primary platforms: Linux and macOS … Windows remains a portability target, not the first runtime-validation gate." | `ZGX\docs\ROADMAP.md:5` |
| **Heavy API churn.** 0.1.0 to 0.1.10 shipped in 11 days (2026-10-25 to 11-04). 436 unreleased `changelog.d` entries since then. 0.1.x is meant to be additive (semver-checked), with breaks queued for 0.2. 36 different zgx git revisions are checked out in `~/.cargo/git`. | `ZGX\CHANGELOG.md:5-817`; `ZGX\changelog.d\` (436 files); `ZGX\docs\ROADMAP.md:73-128`; `ZGX\docs\releasing.md:1-11` |
| **crates.io vs git.** A crates.io build misses zgx's patched winit, wgpu-hal and AccessKit: a covered window keeps drawing, the Windows shader cache is lost, and more. A git dependency must copy the `[patch.crates-io]` block, as the user's app does. | `ZGX\docs\getting-started.md:9-39`; `ZGX\docs\windows.md:66-79,838`; `ZERON\Cargo.toml:33,163-172` |
| **A long-session crash bug was fixed only after 0.1.10.** The patched winit leaked a GDI DC on each DPI read. After 1-2 h, `GetDC returned null` panics. Fatal for hour-long meetings: pin at or after `24b3587`. | `ZGX\changelog.d\windows-dpi-dc-leak.md` |
| **Missing Windows features we need.** No HWND, display affinity, skip-taskbar, non-activating window, tray, global hotkey, autostart or audio. Click-through is whole-window only. | §2 and §3 above; `application.rs:344-522,4753-4755` |
| No rich-text editor; no native Win32 menu bar | `ZGX\docs\rich-text.md:39`; `ZGX\docs\windows.md:842` |
| **Glass caveats.** Mica/Acrylic need Win11 22H2+. Plain acrylic works from Win10 1809 but lets about 83% through. Win10 popups get no blur. With Transparency effects off, glass becomes solid. | `ZGX\docs\window-controls.md:29`; `ZGX\docs\windows.md:473-482,560-566` |
| **Known Windows issues.** Cancelling a file dialog doesn't close it. The Windows text cursor indicator doesn't find the caret. The NVIDIA D3D12 driver can hang with many devices (tests only). | `ZGX\docs\windows.md:634-636,839-841`; `ZGX\docs\validation.md:62-75` |
| **Known zgx limits from the user's own app.** Most open Windows asks appear fixed upstream: caption buttons (`windows.md:494-517`), popup blur (`:545-566`) and reported transparency (`:622-624`). Still open: a 70-360 ms shader-compile freeze on first draw, and no inline nodes. | `ZERON\zgx-asks\README.md:14-18` |
| **Media and IME keys don't reach apps** (fix waits for 0.2). Irrelevant if hotkeys come from `global-hotkey`. | `ZGX\docs\ROADMAP.md:91-108` |
| **Luau build gotchas.** Release builds need `LUAU_CXXFLAGS=-DNDEBUG` in `.cargo\config.toml` (about 20% faster). Rust ≥1.95 and MSVC are required. | `ZGX\docs\scripting.md:631-641`; `ZGX\Cargo.toml` (`rust-version = "1.95"`); `ZGX\docs\windows.md:1-9` |
| **Tooling gaps on this machine.** No `zgx-run` (so `zgx run` and `zgx audit` fail), no `luau-lsp` (so `zgx check` fails). The CLI is at rev `2e2334a`, not the checkout's `24b3587`. | Verified in this session; `C:\Users\HP MEDIA\.cargo\.crates.toml` |

---

## Suggested shape

1. **Process.**
   - One zgx `Application` with `application_id` (single instance and toasts) and `quit_on_last_window_close(false)`.
   - The tray (`tray-icon`) and `global-hotkey` are created on the UI thread. Their events reach a zgx task through a channel.
2. **Rust core.**
   - A tokio runtime on its own threads runs cpal/WASAPI capture ×2, the ElevenLabs websocket, HTTP and `rusqlite`.
   - It talks to the UI through `tokio::sync::mpsc` for transcript segments (lossless) and `zgx::task::latest` for the partial line and levels.
   - A UI task applies them to `host.shared(...)`.
3. **Luau UI.** One `ScriptHost` with two `window_view`s:
   - **Captions:** opened from Rust with `transparent`, `Titlebar::Hidden`, `Glass::…plain()`, `AlwaysOnTop` and `frame_autosave`. Then raw Win32 for `WDA_EXCLUDEFROMCAPTURE` and `WS_EX_TOOLWINDOW`.
   - **Notes:** `ui.tabs`, `ui.list{measured,follow,key}` for the transcript, `ui.textarea` for notes, `ui.markdown_view` for the summary, `ui.modal` for the speaker rename, a `ui.window` for settings.
4. **Transcript rows.** Rows are stored in Rust/SQLite and served through `host.function` / `ui.call_sync`, so Luau never holds the whole meeting.
5. **Dependency.** Pin a git rev at or after `24b3587` with the `[patch.crates-io]` block. Add `rodio` for voice samples.
