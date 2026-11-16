//! Providers from the outside: the registry, the main/backup choice per job,
//! keys in the secret store (never in files), the Test hook and fallback.

use std::sync::Arc;

use recap_engine::{Llm, ProviderErrorKind};
use recap_providers::{
    Draft, Group, Job, MemorySecrets, Providers, Registry, Secrets, Slot, fake, mask,
};

fn registry() -> Registry {
    Registry::builtin().with(fake::BATCH_ONLY, [fake::batch_only_preset()]).with(fake::AI, [fake::ai_preset()])
}

fn providers() -> (Providers, Arc<MemorySecrets>) {
    let secrets = Arc::new(MemorySecrets::default());
    (Providers::open(registry(), None, secrets.clone()), secrets)
}

fn ids(rows: &[serde_json::Value]) -> Vec<&str> {
    rows.iter().map(|r| r["id"].as_str().unwrap()).collect()
}

#[test]
fn speech_providers_come_from_the_registry_with_what_they_can_do() {
    let registry = Registry::builtin();
    for id in ["elevenlabs", "deepgram"] {
        let kind = registry.kind(id).unwrap();
        let c = kind.capabilities;
        assert!(c.live && c.batch && c.diarize && c.keyterms && c.word_timestamps, "{id}");
        assert_eq!(registry.preset(id).unwrap().group, Group::Speech);
    }
}

#[test]
fn every_ai_preset_of_the_add_provider_screen_is_there_in_its_group() {
    let registry = Registry::builtin();
    let ai: Vec<(&str, Group)> =
        registry.presets().iter().filter(|p| p.group != Group::Speech).map(|p| (p.id, p.group)).collect();
    use Group::*;
    assert_eq!(
        ai,
        [
            ("anthropic", Api),
            ("openai", Api),
            ("openrouter", Api),
            ("groq", Api),
            ("deepseek", Api),
            ("mistral", Api),
            ("gemini", Api),
            ("ollama", Local),
            ("claude_code", Cli),
            ("codex", Cli),
            ("gemini_cli", Cli),
            ("openai_compatible", Custom),
            ("custom_command", Custom),
        ]
    );
    for preset in registry.presets() {
        assert!(registry.kind(preset.kind).is_some(), "{} has no kind {}", preset.id, preset.kind);
    }
    let claude = registry.preset("claude_code").unwrap();
    assert_eq!((claude.kind, claude.detail, claude.logo), ("cli", "claude", "claude"));
    assert_eq!(registry.preset("openrouter").unwrap().base_url, "https://openrouter.ai/api/v1");
    assert_eq!(registry.preset("ollama").unwrap().detail, "localhost:11434");
}

#[test]
fn each_job_starts_with_a_main_and_a_backup() {
    let (providers, _) = providers();
    assert_eq!(providers.choice(Job::Live), (Some("elevenlabs".into()), Some("deepgram".into())));
    assert_eq!(providers.choice(Job::Transcript), (Some("elevenlabs".into()), Some("deepgram".into())));
    assert_eq!(providers.choice(Job::Summary), (Some("claude_code".into()), None));
}

#[test]
fn a_job_lists_only_providers_that_can_do_it() {
    let (providers, _) = providers();
    providers.save(Draft::new("fake_ai")).unwrap();
    assert_eq!(providers.eligible(Job::Live), ["elevenlabs", "deepgram"]);
    // The batch-only engine can't caption live, nor name speakers.
    assert_eq!(providers.eligible(Job::Transcript), ["elevenlabs", "deepgram"]);
    assert_eq!(providers.eligible(Job::Summary), ["claude_code", "fake_ai"]);
    let view = providers.view();
    let live = &view["jobs"][0];
    assert_eq!(live["id"], "live");
    assert_eq!(live["label"], "Live captions");
    assert_eq!(live["needs"], serde_json::json!(["live"]));
    assert_eq!(view["jobs"][1]["needs"], serde_json::json!(["batch", "diarize", "word_timestamps"]));
    assert_eq!(view["jobs"][2]["family"], "ai");
    let stt = view["stt"].as_array().unwrap();
    assert_eq!(ids(stt), ["elevenlabs", "deepgram", "batch_only"]);
    assert_eq!(stt[2]["capabilities"]["live"], false);
}

#[test]
fn a_provider_that_cannot_do_the_job_is_refused() {
    let (providers, _) = providers();
    assert!(providers.set_job(Job::Live, Slot::Main, Some("claude_code")).is_err());
    assert!(providers.set_job(Job::Live, Slot::Backup, Some("batch_only")).is_err());
    assert!(providers.set_job(Job::Summary, Slot::Main, Some("elevenlabs")).is_err());
    assert!(providers.set_job(Job::Summary, Slot::Main, None).is_err(), "a job always has a main");
    assert_eq!(providers.choice(Job::Live), (Some("elevenlabs".into()), Some("deepgram".into())));
}

#[test]
fn choosing_the_backup_as_main_swaps_them() {
    let (providers, _) = providers();
    providers.set_job(Job::Live, Slot::Main, Some("deepgram")).unwrap();
    assert_eq!(providers.choice(Job::Live), (Some("deepgram".into()), Some("elevenlabs".into())));
    providers.set_job(Job::Live, Slot::Backup, None).unwrap();
    assert_eq!(providers.choice(Job::Live), (Some("deepgram".into()), None));
}

#[test]
fn keys_go_to_the_secret_store_and_show_masked() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("providers.json");
    let secrets = Arc::new(MemorySecrets::default());
    let providers = Providers::open(registry(), Some(path.clone()), secrets.clone());
    providers.set_key("elevenlabs", "sk_0123456789abcdef3f9a").unwrap();
    let id = providers
        .save(Draft {
            key: Some("sk-or-v1-0123456789abcdef0123456789abcdef91be".into()),
            model: Some("deepseek/deepseek-chat-v3".into()),
            ..Draft::new("openrouter")
        })
        .unwrap();
    assert_eq!(id, "openrouter");
    assert_eq!(secrets.get("elevenlabs").unwrap().as_deref(), Some("sk_0123456789abcdef3f9a"));
    let file = std::fs::read_to_string(&path).unwrap();
    assert!(!file.contains("0123456789abcdef"), "a key reached providers.json: {file}");
    assert!(file.contains("deepseek/deepseek-chat-v3"));
    let view = providers.view();
    assert_eq!(view["stt"][0]["key_mask"], "sk_••••3f9a");
    assert_eq!(view["stt"][1]["key_mask"], "");
    let openrouter = view["ai"].as_array().unwrap().iter().find(|r| r["id"] == "openrouter").unwrap().clone();
    assert_eq!(openrouter["key_mask"], "sk-or-••••91be");
    assert_eq!(openrouter["name"], "OpenRouter");
    assert_eq!(openrouter["subtitle"], "deepseek/deepseek-chat-v3");
    assert_eq!(openrouter["trailing"], "sk-or-••••91be");
    assert!(!view.to_string().contains("0123456789abcdef"), "a key reached the UI");
}

#[test]
fn masks_keep_the_prefix_and_the_last_four() {
    assert_eq!(mask("sk_0123456789abcdef3f9a"), "sk_••••3f9a");
    assert_eq!(mask("sk-or-v1-0123456789abcdef91be"), "sk-or-••••91be");
    assert_eq!(mask("0123456789abcdef0123a21c"), "••••a21c");
    assert_eq!(mask("short"), "••••");
    assert_eq!(mask(""), "");
}

#[test]
fn rows_say_where_a_local_or_command_line_provider_runs() {
    let (providers, _) = providers();
    providers.save(Draft::new("ollama")).unwrap();
    let view = providers.view();
    let ai = view["ai"].as_array().unwrap();
    let claude = ai.iter().find(|r| r["id"] == "claude_code").unwrap();
    assert_eq!((claude["subtitle"].as_str(), claude["trailing"].as_str()), (Some("claude"), Some("")));
    let ollama = ai.iter().find(|r| r["id"] == "ollama").unwrap();
    assert_eq!(ollama["subtitle"], "llama3.1:8b");
    assert_eq!(ollama["trailing"], "localhost:11434");
    assert_eq!(ollama["logo"], "ollama");
}

#[test]
fn the_same_preset_twice_gets_its_own_id_and_editing_keeps_it() {
    let (providers, _) = providers();
    assert_eq!(providers.save(Draft::new("openrouter")).unwrap(), "openrouter");
    assert_eq!(providers.save(Draft::new("openrouter")).unwrap(), "openrouter-2");
    let edited = providers
        .save(Draft { id: Some("openrouter".into()), model: Some("x/y".into()), ..Draft::new("openrouter") })
        .unwrap();
    assert_eq!(edited, "openrouter");
    let view = providers.view();
    let rows: Vec<_> = view["ai"].as_array().unwrap().iter().filter(|r| r["preset"] == "openrouter").collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["subtitle"], "x/y");
}

#[test]
fn a_custom_command_keeps_its_name_command_and_input() {
    let (providers, _) = providers();
    let id = providers
        .save(Draft {
            name: Some("Gemini CLI".into()),
            command: Some("gemini -p {prompt} --output-format json".into()),
            input: Some(recap_providers::TranscriptInput::File),
            ..Draft::new("custom_command")
        })
        .unwrap();
    assert_eq!(id, "gemini-cli");
    let settings = providers.settings(&id).unwrap();
    assert_eq!(settings.command, "gemini -p {prompt} --output-format json");
    assert_eq!(settings.input, recap_providers::TranscriptInput::File);
    let view = providers.view();
    let row = view["ai"].as_array().unwrap().iter().find(|r| r["id"] == id).unwrap().clone();
    assert_eq!((row["name"].as_str(), row["subtitle"].as_str()), (Some("Gemini CLI"), Some("gemini")));
}

#[test]
fn removing_a_provider_takes_it_out_of_its_jobs_and_forgets_its_key() {
    let (providers, secrets) = providers();
    let id = providers.save(Draft { key: Some("k-123456789".into()), ..Draft::new("fake_ai") }).unwrap();
    providers.set_job(Job::Summary, Slot::Backup, Some(&id)).unwrap();
    providers.remove(&id).unwrap();
    assert_eq!(providers.choice(Job::Summary), (Some("claude_code".into()), None));
    assert_eq!(secrets.get(&id).unwrap(), None);
    // Speech providers are code: they stay.
    assert!(providers.remove("elevenlabs").is_err());
}

#[test]
fn choices_and_providers_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("providers.json");
    let secrets = Arc::new(MemorySecrets::default());
    {
        let providers = Providers::open(registry(), Some(path.clone()), secrets.clone());
        let id = providers.save(Draft::new("fake_ai")).unwrap();
        providers.set_job(Job::Summary, Slot::Backup, Some(&id)).unwrap();
        providers.set_job(Job::Live, Slot::Main, Some("deepgram")).unwrap();
    }
    let providers = Providers::open(registry(), Some(path), secrets);
    assert_eq!(providers.choice(Job::Summary), (Some("claude_code".into()), Some("fake_ai".into())));
    assert_eq!(providers.choice(Job::Live), (Some("deepgram".into()), Some("elevenlabs".into())));
}

#[tokio::test]
async fn test_says_ok_with_the_latency() {
    let (providers, _) = providers();
    let id = providers.save(Draft::new("fake_ai")).unwrap();
    let outcome = providers.test(Draft { id: Some(id.clone()), ..Draft::new("fake_ai") }).await;
    assert!(outcome.ok, "{outcome:?}");
    assert!(outcome.error.is_none());
    // A tested provider shows it on its row.
    let view = providers.view();
    let row = view["ai"].as_array().unwrap().iter().find(|r| r["id"] == id).unwrap().clone();
    assert_eq!(row["tested"]["ok"], true);
}

#[tokio::test]
async fn test_says_what_went_wrong() {
    let (providers, _) = providers();
    let outcome = providers.test(Draft { model: Some("quota".into()), ..Draft::new("fake_ai") }).await;
    assert!(!outcome.ok);
    assert_eq!(outcome.error.as_deref(), Some("Out of credit (fake)"));
}

#[tokio::test]
async fn test_without_a_key_asks_for_one() {
    let (providers, _) = providers();
    let outcome = providers.test(Draft::new("openrouter")).await;
    assert!(!outcome.ok);
    assert_eq!(outcome.error.as_deref(), Some("Add an API key"));
    // A typed (unsaved) key is what is tested.
    let outcome = providers.test(Draft { key: Some("sk-typed".into()), ..Draft::new("anthropic") }).await;
    assert_ne!(outcome.error.as_deref(), Some("Add an API key"));
}

#[tokio::test]
async fn adapters_not_built_yet_say_so() {
    let (providers, secrets) = providers();
    secrets.set("elevenlabs", "sk_test").unwrap();
    let outcome = providers.test(Draft { id: Some("elevenlabs".into()), ..Draft::new("elevenlabs") }).await;
    assert!(!outcome.ok);
    assert!(outcome.error.unwrap().contains("not built yet"));
}

#[tokio::test]
async fn the_summary_job_falls_back_to_its_backup() {
    let (providers, _) = providers();
    let main = providers.save(Draft { name: Some("Main".into()), model: Some("auth".into()), ..Draft::new("fake_ai") }).unwrap();
    let backup = providers.save(Draft { name: Some("Backup".into()), ..Draft::new("fake_ai") }).unwrap();
    providers.set_job(Job::Summary, Slot::Main, Some(&main)).unwrap();
    providers.set_job(Job::Summary, Slot::Backup, Some(&backup)).unwrap();
    let switched = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = switched.clone();
    let llm = providers.llm(move |notice| seen.lock().unwrap().push(notice)).unwrap();
    let answer = llm.complete_json("p".into(), serde_json::json!({})).await.unwrap();
    assert_eq!(answer["provider"], "Backup");
    let switched = switched.lock().unwrap();
    assert_eq!((switched[0].from.as_str(), switched[0].to.as_str()), ("Main", "Backup"));
    assert_eq!(switched[0].error.kind, ProviderErrorKind::Auth);
}

#[test]
fn a_job_whose_adapter_is_not_built_says_so() {
    let (providers, secrets) = providers();
    let error = providers.live(|_| {}).err().unwrap();
    assert_eq!(error.kind, ProviderErrorKind::Auth, "no key yet: {error:?}");
    secrets.set("elevenlabs", "sk_test").unwrap();
    let error = providers.live(|_| {}).err().unwrap();
    assert!(error.message.contains("not built yet"), "{error:?}");
}
