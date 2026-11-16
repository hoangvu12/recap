//! Settings through the real UI (ui/app.luau, headless) and the real host
//! side (recap::settings) on fake providers: what the Providers page does
//! reaches recap_providers, and its answers come back to the page.

use std::sync::Arc;

use recap::{host, settings::ProvidersHost};
use recap_providers::{Draft, Job, MemorySecrets, Providers, Registry, Secrets, fake};
use zgx_script::{ScriptHost, testing::TestApp};

struct Fixture {
    app: TestApp,
    providers: Providers,
    secrets: Arc<MemorySecrets>,
    _runtime: tokio::runtime::Runtime,
}

/// Settings open on Providers, over `setup`'s providers.
fn settings(setup: impl FnOnce(&Providers)) -> (Fixture, TestApp) {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    let secrets = Arc::new(MemorySecrets::default());
    let registry = Registry::builtin().with(fake::AI, [fake::ai_preset()]);
    let providers = Providers::open(registry, None, secrets.clone());
    setup(&providers);
    let (p, handle) = (providers.clone(), runtime.handle().clone());
    let app = TestApp::new(move |rt| {
        let script = ScriptHost::new(rt, host::script_path());
        script.shared("host.connected", true);
        ProvidersHost::register(&script, p, handle);
        script.shared("window.settings", true);
        script
    });
    app.frames(2);
    let mut win = app.window("settings").expect("Settings is open");
    win.click("#nav-providers");
    assert!(win.wait_until(|w| w.find("#providers-page").is_some()));
    (Fixture { app, providers, secrets, _runtime: runtime }, win)
}

#[test]
fn the_page_shows_the_registry_and_each_jobs_choice() {
    let (_f, mut win) = settings(|_| {});
    assert_eq!(win.text("#job-live-main-name"), "ElevenLabs");
    assert_eq!(win.text("#job-live-backup-name"), "Deepgram");
    assert_eq!(win.text("#job-summary-main-name"), "Claude Code");
    assert_eq!(win.text("#stt-deepgram-ability-2"), "Speakers");
    assert_eq!(win.text("#ai-claude_code-subtitle"), "claude");
}

#[test]
fn choosing_a_main_in_the_picker_changes_the_job() {
    let (f, mut win) = settings(|_| {});
    win.click("#job-live-main");
    assert!(win.wait_until(|w| w.find("#pick-live-main-deepgram").is_some()));
    win.click("#pick-live-main-deepgram");
    assert!(win.wait_until(|w| w.text("#job-live-main-name") == "Deepgram"));
    assert_eq!(f.providers.choice(Job::Live), (Some("deepgram".into()), Some("elevenlabs".into())));
    assert_eq!(win.text("#job-live-backup-name"), "ElevenLabs");
    drop(f.app);
}

#[test]
fn a_key_typed_in_change_goes_to_the_secret_store_and_shows_masked() {
    let (f, mut win) = settings(|_| {});
    win.click("#stt-elevenlabs-change");
    assert!(win.wait_until(|w| w.find("#form-key").is_some()));
    win.type_text("#form-key", "sk_0123456789abcdef3f9a");
    win.click("#form-save");
    assert!(win.wait_until(|w| w.text("#stt-elevenlabs-key") == "sk_••••3f9a"));
    assert_eq!(f.secrets.get("elevenlabs").unwrap().as_deref(), Some("sk_0123456789abcdef3f9a"));
}

#[test]
fn test_runs_the_providers_test_and_shows_the_outcome() {
    let mut id = String::new();
    let (_f, mut win) = settings(|p| {
        id = p.save(Draft { model: Some("quota".into()), ..Draft::new("fake_ai") }).unwrap();
    });
    win.click(&format!("#ai-{id}"));
    assert!(win.wait_until(|w| w.find("#form-test").is_some()));
    win.click("#form-test");
    assert!(win.wait_until(|w| {
        w.find("#form-test-result").is_some() && w.text("#form-test-result") == "Out of credit (fake)"
    }));
}
