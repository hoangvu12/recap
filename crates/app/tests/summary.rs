//! Seam 1 after Stop, the AI half: the test WAV recorded through a live
//! session, transcribed by a fake batch STT (the recorded ElevenLabs
//! answers), then named and summarised by a fake `Llm` that answers from
//! fixtures (tests/fixtures/llm_*.json). What the steps ask, what they keep
//! and what is left on disk.

mod recorded;

use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};

use recap::{
    naming::{self, NAMES},
    processing::{self, Processing, ProcessingConfig, ProcessingEvent, Progress, StepId, StepStatus},
    summary::{self, SummaryData},
};
use recap_engine::{
    BatchRequest, BatchStt, BatchTranscript, BoxFuture, Capabilities, Llm, ProviderError, ProviderErrorKind, Source,
};
use recap_storage::{MeetingId, Segment, Speaker, Status, Store};
use serde_json::{Value, json};

fn fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap()
}

/// Answers like ElevenLabs did on the test meeting.
struct FakeBatch;

impl BatchStt for FakeBatch {
    fn name(&self) -> &str {
        "Fake"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { batch: true, diarize: true, keyterms: true, word_timestamps: true, live: false }
    }
    fn transcribe(&self, request: BatchRequest) -> BoxFuture<'static, Result<BatchTranscript, ProviderError>> {
        let answer: BatchTranscript =
            serde_json::from_value(fixture(if request.diarize { "stt_them.json" } else { "stt_me.json" })).unwrap();
        Box::pin(async move { Ok(answer) })
    }
}

/// What the fake AI was asked.
#[derive(Clone, Debug)]
struct Asked {
    prompt: String,
    schema: Value,
}

impl Asked {
    fn is_summary(&self) -> bool {
        self.schema["properties"].get("overview").is_some()
    }
}

/// An AI provider answering from fixtures: the naming answer for a naming
/// schema, the summary for a summary one. It can fail the next summary.
#[derive(Clone)]
struct FakeLlm {
    naming: Value,
    summary: Value,
    asked: Arc<Mutex<Vec<Asked>>>,
    fail_summary: Arc<Mutex<Option<ProviderError>>>,
}

impl FakeLlm {
    fn new(naming: Value, summary: Value) -> Self {
        Self { naming, summary, asked: Arc::default(), fail_summary: Arc::default() }
    }

    fn fixtures() -> Self {
        Self::new(fixture("llm_naming.json"), fixture("llm_summary.json"))
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    fn naming_calls(&self) -> usize {
        self.asked().iter().filter(|a| !a.is_summary()).count()
    }

    fn summary_calls(&self) -> usize {
        self.asked().iter().filter(|a| a.is_summary()).count()
    }
}

impl Llm for FakeLlm {
    fn name(&self) -> &str {
        "Fake AI"
    }

    fn complete_json(&self, prompt: String, schema: Value) -> BoxFuture<'static, Result<Value, ProviderError>> {
        let asked = Asked { prompt, schema: schema.clone() };
        let is_summary = asked.is_summary();
        self.asked.lock().unwrap().push(asked);
        let outcome = if is_summary {
            match self.fail_summary.lock().unwrap().take() {
                Some(error) => Err(error),
                None => Ok(self.summary.clone()),
            }
        } else {
            Ok(self.naming.clone())
        };
        // What a real provider gives back has been held to the schema.
        if let Ok(value) = &outcome {
            fits(value, &schema, "$");
        }
        Box::pin(async move { outcome })
    }
}

/// The schema rules #10 asks for (every object `required` + closed), and
/// the answer fitting it.
fn fits(value: &Value, schema: &Value, path: &str) {
    let types: Vec<&str> = match &schema["type"] {
        Value::String(t) => vec![t.as_str()],
        Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if let Some(allowed) = schema["enum"].as_array() {
        assert!(allowed.contains(value), "{path}: {value} not in {allowed:?}");
    }
    match value {
        Value::Object(object) => {
            assert!(types.contains(&"object"), "{path}: an object where {types:?}");
            assert_eq!(schema["additionalProperties"], json!(false), "{path}: closed");
            let properties = schema["properties"].as_object().expect("properties");
            let required: Vec<&str> = schema["required"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            let mut names: Vec<&str> = properties.keys().map(String::as_str).collect();
            names.sort();
            let mut sorted = required.clone();
            sorted.sort();
            assert_eq!(sorted, names, "{path}: every property required");
            for name in &required {
                assert!(object.contains_key(*name), "{path}.{name}: missing");
            }
            for (name, item) in object {
                let property = properties.get(name).unwrap_or_else(|| panic!("{path}.{name}: not in the schema"));
                fits(item, property, &format!("{path}.{name}"));
            }
        }
        Value::Array(items) => {
            assert!(types.contains(&"array"), "{path}: an array where {types:?}");
            for (i, item) in items.iter().enumerate() {
                fits(item, &schema["items"], &format!("{path}[{i}]"));
            }
        }
        Value::String(_) => assert!(types.contains(&"string"), "{path}: a string where {types:?}"),
        Value::Null => assert!(types.contains(&"null"), "{path}: null where {types:?}"),
        Value::Bool(_) => assert!(types.contains(&"boolean"), "{path}"),
        Value::Number(_) => assert!(types.contains(&"number") || types.contains(&"integer"), "{path}"),
    }
}

fn config(my_name: &str) -> ProcessingConfig {
    ProcessingConfig { keyterms: vec![], my_name: my_name.into() }
}

fn full(llm: &FakeLlm) -> Processing {
    Processing::full(Ok(Arc::new(FakeBatch) as Arc<dyn BatchStt>), Ok(Arc::new(llm.clone()) as Arc<dyn Llm>))
}

async fn process(processing: &Processing, store: &Store, id: &MeetingId, config: ProcessingConfig) -> Vec<ProcessingEvent> {
    let mut events = processing.start(store.clone(), id.clone(), config);
    let mut out = Vec::new();
    while let Some(event) = events.recv().await {
        out.push(event);
    }
    out
}

fn last_progress(events: &[ProcessingEvent]) -> Progress {
    events
        .iter()
        .rev()
        .find_map(|e| match e {
            ProcessingEvent::Progress(p) => Some(p.clone()),
            _ => None,
        })
        .expect("progress was told")
}

fn names(store: &Store, id: &MeetingId) -> Vec<(String, Option<String>)> {
    let info = store.meeting(id).unwrap().unwrap().info;
    info.speakers.into_iter().map(|s| (s.id, s.name)).collect()
}

fn some(id: &str, name: &str) -> (String, Option<String>) {
    (id.into(), Some(name.into()))
}

fn unnamed(id: &str) -> (String, Option<String>) {
    (id.into(), None)
}

#[tokio::test(flavor = "multi_thread")]
async fn after_stop_the_speakers_are_named_and_the_summary_written() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = recorded::record(&store).await;
    store.set_my_notes(&info.id, "Ask about the India fallback").unwrap();
    let llm = FakeLlm::fixtures();
    let events = process(&full(&llm), &store, &info.id, config("Vu")).await;

    let Some(ProcessingEvent::Finished(done)) = events.last() else { panic!("finished: {:?}", events.last()) };
    assert_eq!(done.status, Status::Done);
    let steps: Vec<StepId> = events
        .iter()
        .filter_map(|e| match e {
            ProcessingEvent::StepDone(step) => Some(*step),
            _ => None,
        })
        .collect();
    assert_eq!(steps, StepId::ALL, "each step told as it finished");
    assert!(last_progress(&events).finished());

    // Naming: asked once, about the voices by id, with the transcript.
    let asked = llm.asked();
    assert_eq!((llm.naming_calls(), llm.summary_calls()), (1, 1));
    assert!(!asked[0].is_summary() && asked[1].is_summary(), "names first, then the summary");
    let naming = &asked[0];
    assert_eq!(naming.schema["required"], json!(["S1", "S2", "S3"]));
    assert!(naming.prompt.contains("S2: Thanks, Tom."), "turns carry their speaker id: {}", naming.prompt);
    assert!(naming.prompt.contains("Vu"), "it knows who Me is");

    // The names given; the transcript's turns stay theirs.
    assert_eq!(names(&store, &info.id), [some("S1", "Tom"), some("S2", "Priya"), some("S3", "David")]);
    assert_eq!(done.speakers, store.meeting(&info.id).unwrap().unwrap().info.speakers);
    let kept: Value = serde_json::from_slice(&std::fs::read(store.folder(&info.id).join(NAMES)).unwrap()).unwrap();
    assert_eq!(kept, fixture("llm_naming.json"), "the answer is kept beside the meeting");

    // The summary: asked with the named transcript and my notes.
    let summary_prompt = &asked[1].prompt;
    assert!(summary_prompt.contains("Priya: Thanks, Tom."), "{summary_prompt}");
    assert!(summary_prompt.contains("David: From the backend side"), "{summary_prompt}");
    assert!(summary_prompt.contains("Ask about the India fallback"), "my notes steer it");

    // Kept: the data, its Markdown (summary.md), the overview, the title.
    let meeting = store.meeting(&info.id).unwrap().unwrap();
    assert_eq!(meeting.info.title, "Mobile app launch on Dec 1");
    let kept = meeting.summary.expect("a summary");
    assert!(kept.overview.starts_with("Tom, Priya and David agreed"));
    let data: SummaryData = serde_json::from_value(kept.data.clone()).unwrap();
    assert_eq!(data.next_steps.len(), 5);
    assert!(data.next_steps.iter().all(|s| !s.done));
    let md = std::fs::read_to_string(store.folder(&info.id).join("summary.md")).unwrap();
    assert_eq!(md, kept.markdown);
    assert!(md.contains("### Launch readiness\n- Design: onboarding done"), "{md}");
    assert!(md.contains("### Next steps\n- [ ] **David**: Add caching and rerun the load test (Before Dec 1)\n"), "{md}");
    assert!(md.contains("### Decisions\n- Launch on December 1, with caching in place.\n"), "{md}");
    // Home finds it by what the summary says.
    assert_eq!(store.search("fallback").unwrap(), std::slice::from_ref(&info.id));
}

#[tokio::test(flavor = "multi_thread")]
async fn names_come_only_from_evidence_in_the_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::stopped(&store);
    // Every way a name could be wrong, beside one good one.
    let guesses = json!({
        // Evidence nobody said.
        "S1": { "name": "Mark", "evidence": "Hi, I'm Mark from sales.", "confidence": "high" },
        // Good.
        "S2": { "name": "Priya", "evidence": "Priya, can design finish the dark mode work by then?", "confidence": "high" },
        // Not sure enough.
        "S3": { "name": "David", "evidence": "Thanks for joining, Priya and David.", "confidence": "low" }
    });
    let llm = FakeLlm::new(guesses, fixture("llm_summary.json"));
    let events = process(&full(&llm), &store, &id, config("")).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Finished(_))), "{:?}", events.last());
    assert_eq!(names(&store, &id), [unnamed("S1"), some("S2", "Priya"), unnamed("S3")]);
}

fn guess(name: Option<&str>, evidence: &str, confidence: &str) -> naming::Guess {
    serde_json::from_value(json!({ "name": name, "evidence": evidence, "confidence": confidence })).unwrap()
}

#[test]
fn a_name_is_taken_only_when_it_is_sure_said_and_nobody_elses() {
    let said = "Okay, let's get started. Thanks for joining, Priya and David. Thanks, Tom. Priya, can you finish?";
    let cases: Vec<(&str, naming::Guess, Option<&str>)> = vec![
        ("sure and said", guess(Some("Tom"), "Thanks, Tom.", "high"), Some("Tom")),
        ("the quote differs in case and punctuation only", guess(Some("Tom"), "thanks tom", "high"), Some("Tom")),
        // Claude Code quotes the lines as it was shown them (seen for real).
        ("the quote keeps the lines' time and label", guess(Some("Tom"), "[0:11] S2: Thanks, Tom.", "high"), Some("Tom")),
        (
            "a quote over two lines",
            guess(Some("Priya"), "[0:38] S1: Thanks, Tom. [0:45] Speaker 2: Priya, can you finish?", "high"),
            Some("Priya"),
        ),
        ("no name", guess(None, "", "low"), None),
        ("not sure enough", guess(Some("Tom"), "Thanks, Tom.", "medium"), None),
        ("evidence not in the transcript", guess(Some("Tom"), "I'm Tom.", "high"), None),
        ("the name is not in its evidence", guess(Some("Tom"), "Thanks for joining, Priya and David.", "high"), None),
        ("a placeholder", guess(Some("Speaker 1"), "Thanks, Tom.", "high"), None),
        ("my name: I am on the mic, not a voice of Them", guess(Some("Vu"), "Thanks, Tom.", "high"), None),
    ];
    for (why, g, want) in cases {
        let guesses = BTreeMap::from([("S1".to_string(), g)]);
        let accepted = naming::accept(&guesses, said, "Vu");
        assert_eq!(accepted.get("S1").map(String::as_str), want, "{why}");
    }
    // Two voices with one name: neither gets it.
    let guesses = BTreeMap::from([
        ("S1".to_string(), guess(Some("Priya"), "Priya, can you finish?", "high")),
        ("S2".to_string(), guess(Some("priya"), "Thanks for joining, Priya and David.", "high")),
        ("S3".to_string(), guess(Some("David"), "Thanks for joining, Priya and David.", "high")),
    ]);
    let accepted = naming::accept(&guesses, said, "");
    assert_eq!(accepted.into_iter().collect::<Vec<_>>(), [("S3".to_string(), "David".to_string())]);
}

/// A meeting transcribed already (step 1 done) with `segments` and `speakers`.
fn transcribed(store: &Store, segments: &[Segment], speakers: Vec<Speaker>) -> MeetingId {
    let id = recorded::stopped(store);
    store.set_refined(&id, segments).unwrap();
    store.update(&id, |info| info.speakers = speakers).unwrap();
    let mut progress = Progress::default();
    progress.steps[0].status = StepStatus::Done;
    std::fs::write(store.folder(&id).join(processing::PROGRESS), serde_json::to_vec(&progress).unwrap()).unwrap();
    id
}

fn segment(source: Source, speaker: &str, at: u64, text: &str) -> Segment {
    Segment { source, speaker: Some(speaker.into()), start_ms: at, end_ms: at + 900, text: text.into() }
}

#[tokio::test(flavor = "multi_thread")]
async fn me_is_my_name_and_names_given_before_stay() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = transcribed(
        &store,
        &[
            segment(Source::Me, "me", 0, "Thanks for joining, Priya."),
            segment(Source::Them, "S1", 1000, "Happy to be here."),
            segment(Source::Them, "S2", 2000, "Me too."),
        ],
        vec![
            Speaker { id: "me".into(), name: None },
            Speaker { id: "S1".into(), name: None },
            Speaker { id: "S2".into(), name: Some("Lan".into()) },
        ],
    );
    let guesses = json!({
        "S1": { "name": "Priya", "evidence": "Thanks for joining, Priya.", "confidence": "high" },
        "S2": { "name": null, "evidence": "", "confidence": "low" }
    });
    let llm = FakeLlm::new(guesses, fixture("llm_summary.json"));
    let events = process(&full(&llm), &store, &id, config("Vu")).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Finished(_))), "{:?}", events.last());
    assert_eq!(names(&store, &id), [some("me", "Vu"), some("S1", "Priya"), some("S2", "Lan")]);
    let naming = &llm.asked()[0];
    assert!(naming.prompt.contains("Vu: Thanks for joining, Priya."), "Me's lines carry my name: {}", naming.prompt);
    assert_eq!(naming.schema["required"], json!(["S1", "S2"]), "Me is not asked about");
}

#[tokio::test(flavor = "multi_thread")]
async fn with_every_voice_named_already_naming_asks_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = transcribed(
        &store,
        &[segment(Source::Them, "S1", 0, "We launch on December 1.")],
        vec![Speaker { id: "S1".into(), name: Some("Tom".into()) }],
    );
    let llm = FakeLlm::fixtures();
    let events = process(&full(&llm), &store, &id, config("")).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Finished(_))), "{:?}", events.last());
    assert_eq!((llm.naming_calls(), llm.summary_calls()), (0, 1));
    assert!(llm.asked()[0].prompt.contains("Tom: We launch on December 1."));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_summary_says_why_and_retry_resumes_from_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::stopped(&store);
    let llm = FakeLlm::fixtures();
    *llm.fail_summary.lock().unwrap() = Some(ProviderError::new(ProviderErrorKind::Quota, "Claude usage limit reached"));
    let processing = full(&llm);

    let events = process(&processing, &store, &id, config("")).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Stopped(e)) if e == "Claude usage limit reached"), "{:?}", events.last());
    let progress = last_progress(&events);
    let statuses: Vec<_> = progress.steps.iter().map(|s| s.status).collect();
    assert_eq!(statuses, [StepStatus::Done, StepStatus::Done, StepStatus::Error]);
    let meeting = store.meeting(&id).unwrap().unwrap();
    assert_eq!(meeting.info.status, Status::Processing, "unfinished");
    assert!(meeting.summary.is_none());
    // The names are kept.
    assert_eq!(names(&store, &id)[0], some("S1", "Tom"));

    // Retry: only the summary is asked for again.
    let events = process(&processing, &store, &id, config("")).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Finished(_))), "{:?}", events.last());
    assert_eq!((llm.naming_calls(), llm.summary_calls()), (1, 2));
    assert!(store.meeting(&id).unwrap().unwrap().summary.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn without_an_ai_provider_naming_fails_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::stopped(&store);
    let none = Err(ProviderError::new(ProviderErrorKind::Other, "Summary and names has no provider"));
    let processing = Processing::full(Ok(Arc::new(FakeBatch) as Arc<dyn BatchStt>), none);
    let events = process(&processing, &store, &id, config("")).await;
    let progress = last_progress(&events);
    let failed = progress.failed().unwrap();
    assert_eq!((failed.id, failed.error.as_str()), (StepId::NamingSpeakers, "Summary and names has no provider"));
}

#[tokio::test(flavor = "multi_thread")]
async fn regenerate_fills_a_meeting_processed_before_names_and_summaries() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::stopped(&store);
    // Processed with the empty slots: done, unnamed, no summary.
    let before = Processing::standard(Ok(Arc::new(FakeBatch) as Arc<dyn BatchStt>));
    process(&before, &store, &id, config("")).await;
    assert_eq!(store.meeting(&id).unwrap().unwrap().info.status, Status::Done);
    assert!(store.meeting(&id).unwrap().unwrap().summary.is_none());

    // Regenerate: naming and the summary run again; the transcript stays.
    processing::redo(&store.folder(&id), &[StepId::NamingSpeakers, StepId::WritingSummary]).unwrap();
    let progress = Progress::read(&store.folder(&id));
    let statuses: Vec<_> = progress.steps.iter().map(|s| s.status).collect();
    assert_eq!(statuses, [StepStatus::Done, StepStatus::Pending, StepStatus::Pending]);
    let llm = FakeLlm::fixtures();
    let events = process(&full(&llm), &store, &id, config("")).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Finished(_))), "{:?}", events.last());
    let meeting = store.meeting(&id).unwrap().unwrap();
    assert_eq!(meeting.info.title, "Mobile app launch on Dec 1");
    assert!(meeting.summary.is_some());
    assert_eq!(names(&store, &id)[2], some("S3", "David"));
}

#[tokio::test(flavor = "multi_thread")]
async fn ticked_next_steps_persist_and_survive_a_regenerate() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::stopped(&store);
    let llm = FakeLlm::fixtures();
    process(&full(&llm), &store, &id, config("")).await;

    summary::set_done(&store, &id, 2, true).unwrap();
    let kept = store.meeting(&id).unwrap().unwrap().summary.unwrap();
    assert!(kept.markdown.contains("- [x] **Priya**: Fix dark mode colours on the payment page (Fri, Nov 20)"), "{}", kept.markdown);
    assert_eq!(kept.data["next_steps"][2]["done"], json!(true));
    // A restart reads it back from the folder.
    drop(store);
    for file in ["app.db", "app.db-wal", "app.db-shm"] {
        let _ = std::fs::remove_file(dir.path().join(file));
    }
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.meeting(&id).unwrap().unwrap().summary.unwrap().data["next_steps"][2]["done"], json!(true));
    // Unticked.
    summary::set_done(&store, &id, 2, false).unwrap();
    assert!(store.meeting(&id).unwrap().unwrap().summary.unwrap().markdown.contains("- [ ] **Priya**: Fix dark mode"));
    summary::set_done(&store, &id, 4, true).unwrap();
    assert!(summary::set_done(&store, &id, 9, true).is_err(), "no such step");

    // Regenerating keeps the tick of a step it writes again.
    processing::redo(&store.folder(&id), &[StepId::WritingSummary]).unwrap();
    process(&full(&llm), &store, &id, config("")).await;
    let data = store.meeting(&id).unwrap().unwrap().summary.unwrap().data;
    let done: Vec<bool> = data["next_steps"].as_array().unwrap().iter().map(|s| s["done"].as_bool().unwrap_or(false)).collect();
    assert_eq!(done, [false, false, false, false, true]);
}

#[test]
fn the_summary_tab_reads_the_view() {
    let data: SummaryData = serde_json::from_value(fixture("llm_summary.json")).unwrap();
    let speakers = vec![
        Speaker { id: "S1".into(), name: Some("Tom".into()) },
        Speaker { id: "S2".into(), name: Some("Priya".into()) },
        Speaker { id: "S3".into(), name: None },
    ];
    let view = summary::view("20261116-100000", &data, &speakers);
    assert_eq!(view["id"], "20261116-100000");
    assert_eq!(view["overview"], json!(data.overview));
    assert_eq!(
        view["topics"][1],
        json!({ "heading": "Release notes", "bullets": ["Nobody owned them last time and they shipped late. Tom owns them now."] })
    );
    // Owners take their speaker's chip hue (place from 1; 0 when not a speaker).
    assert_eq!(
        view["next_steps"][4],
        json!({ "index": 5, "owner": "Tom", "owner_n": 1, "text": "Write release notes and share a draft", "due": "Wed, Nov 18", "done": false })
    );
    assert_eq!(view["next_steps"][2]["owner_n"], 2);
    assert_eq!(view["next_steps"][0]["owner_n"], 0, "David is not a named speaker here");
    assert_eq!(view["decisions"].as_array().unwrap().len(), 2);
    assert_eq!(summary::view("", &SummaryData::default(), &[])["next_steps"], json!([]));
}

#[test]
fn markdown_has_the_overview_topics_next_steps_and_decisions() {
    let mut data: SummaryData = serde_json::from_value(json!({
        "title": "T",
        "overview": "We met.",
        "topics": [{ "heading": "Launch", "bullets": ["Dec 1."] }],
        "next_steps": [
            { "owner": "David", "text": "Add caching", "due": null },
            { "owner": null, "text": "Tell support", "due": "Monday" }
        ],
        "decisions": []
    }))
    .unwrap();
    data.next_steps[1].done = true;
    assert_eq!(
        summary::markdown(&data),
        "We met.\n\n### Launch\n- Dec 1.\n\n### Next steps\n- [ ] **David**: Add caching\n- [x] Tell support (Monday)\n"
    );
}

#[test]
fn the_schemas_follow_the_json_rules() {
    // Every object closed and fully required, as Anthropic's structured
    // outputs need (FakeLlm also checks the fixtures against them).
    fn closed(schema: &Value, path: &str) {
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            assert_eq!(schema["additionalProperties"], json!(false), "{path}");
            assert_eq!(schema["required"].as_array().unwrap().len(), properties.len(), "{path}");
            for (name, p) in properties {
                closed(p, &format!("{path}.{name}"));
            }
        }
        if let Some(items) = schema.get("items") {
            closed(items, &format!("{path}[]"));
        }
    }
    closed(&summary::schema(), "$");
    closed(&naming::schema(&["S1", "S2"]), "$");
    assert_eq!(naming::schema(&["S1", "S2"])["required"], json!(["S1", "S2"]));
}
