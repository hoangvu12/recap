//! The acceptance test of CLAUDE.md, for real: the test WAV recorded
//! through a live meeting, then processed by ElevenLabs batch and the
//! installed Claude Code CLI (the user's own login). Ignored unless asked
//! for; skipped without `ELEVENLABS_API_KEY`.
//!
//! `cargo test -p recap --test summary_real -- --ignored --nocapture`
//!
//! Tom, Priya and David are named or "Speaker N", never a wrong name; the
//! summary has the Dec 1 launch, the India soft-launch fallback and the 5
//! next steps with the right owners.

mod recorded;

use std::sync::Arc;

use recap::{
    processing::{Processing, ProcessingConfig, ProcessingEvent},
    summary::SummaryData,
};
use recap_engine::{BatchStt, Llm};
#[allow(unused_imports)]
use recap_providers::Provider;
use recap_providers::{ProviderSettings, Registry, elevenlabs};
use recap_storage::{Status, Store};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "real APIs: ElevenLabs (ELEVENLABS_API_KEY) and the Claude Code CLI"]
async fn the_test_meeting_is_named_and_summarised_for_real() {
    let Some(key) = std::env::var("ELEVENLABS_API_KEY").ok().filter(|k| !k.trim().is_empty()) else {
        eprintln!("ELEVENLABS_API_KEY is not set: skipped");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::record(&store).await.id;

    let batch: Arc<dyn BatchStt> =
        Arc::new(elevenlabs::ElevenLabs::new("ElevenLabs", key.trim(), elevenlabs::Config::default()));
    let registry = Registry::builtin();
    let preset = registry.preset("claude_code").unwrap();
    let settings =
        ProviderSettings { name: "Claude Code".into(), command: preset.command.into(), ..Default::default() };
    let llm: Arc<dyn Llm> = (registry.kind("cli").unwrap().build)(&settings).llm().unwrap();
    // The user is not one of the three (the WAV's mic side is only echo).
    let config = ProcessingConfig {
        keyterms: vec!["Priya".into(), "David".into(), "Tom".into()],
        my_name: "Vu".into(),
        language: None,
    };
    let started = std::time::Instant::now();
    let mut events = Processing::full(Ok(batch), Ok(llm)).start(store.clone(), id.clone(), config);
    let mut last = None;
    while let Some(event) = events.recv().await {
        if let ProcessingEvent::Progress(p) = &event {
            eprintln!(
                "{:?}",
                p.steps.iter().map(|s| (s.id.as_str(), s.status, s.took_ms, s.error.as_str())).collect::<Vec<_>>()
            );
        }
        last = Some(event);
    }
    eprintln!("processed in {:?}", started.elapsed());
    let Some(ProcessingEvent::Finished(info)) = last else { panic!("finished: {last:?}") };
    assert_eq!(info.status, Status::Done);

    // Names: each voice its own name, or Speaker N; never another's. The
    // engine numbers voices as they first speak: Tom, Priya, David.
    let truth = [("S1", "Tom"), ("S2", "Priya"), ("S3", "David")];
    eprintln!("speakers: {:?}", info.speakers);
    eprintln!(
        "names.json: {}",
        std::fs::read_to_string(store.folder(&id).join(recap::naming::NAMES)).unwrap_or_default()
    );
    for speaker in &info.speakers {
        let Some(name) = &speaker.name else { continue };
        let right = truth.iter().find(|(id, _)| *id == speaker.id).map(|(_, n)| *n);
        assert_eq!(Some(name.as_str()), right, "{} named {name}", speaker.id);
    }
    let named = info.speakers.iter().filter(|s| s.name.is_some()).count();
    eprintln!("{named} of {} voices named", info.speakers.len());

    // The summary.
    let meeting = store.meeting(&id).unwrap().unwrap();
    let summary = meeting.summary.expect("a summary");
    eprintln!("title: {}\n{}", meeting.info.title, summary.markdown);
    assert!(!meeting.info.title.is_empty());
    let md = summary.markdown.to_lowercase();
    assert!(md.contains("dec 1") || md.contains("december 1"), "the Dec 1 launch");
    assert!(md.contains("india") && md.contains("soft"), "the India soft-launch fallback");

    let data: SummaryData = serde_json::from_value(summary.data).unwrap();
    // Each of the five, owned by whoever took it (by name or by label).
    let label = |voice: &str| {
        let (id, name) = truth.iter().find(|(_, n)| *n == voice).unwrap();
        let speaker = info.speakers.iter().find(|s| s.id == *id);
        (name.to_string(), speaker.map(recap::home::speaker_label).unwrap_or_default())
    };
    let tasks: [(&str, &[&str]); 5] = [
        ("David", &["caching"]),
        ("David", &["report"]),
        ("Priya", &["dark mode"]),
        ("Priya", &["support"]),
        ("Tom", &["release notes"]),
    ];
    for (owner, words) in tasks {
        let step = data
            .next_steps
            .iter()
            .find(|s| words.iter().all(|w| s.text.to_lowercase().contains(w)))
            .unwrap_or_else(|| panic!("a next step about {words:?}: {:?}", data.next_steps));
        let (name, label) = label(owner);
        let got = step.owner.clone().unwrap_or_default();
        assert!(got == name || got == label, "{:?} is {owner}'s, not {got}", step.text);
    }
    assert_eq!(data.next_steps.len(), 5, "five next steps: {:?}", data.next_steps);
}
