//! Processing (GLOSSARY): the after-Stop pipeline of a stored meeting,
//! Transcribing → Naming speakers → Writing summary. No zgx: the host shows
//! its events.
//!
//! It is a resumable job queue. Each [`Step`] keeps its output in the
//! meeting's folder and the store, and the queue keeps where it is in
//! `processing.json` ([`Progress`]) after every change. Running it again
//! ([`Processing::start`]: Retry after an error, Recover after a crash) skips
//! the steps already done and starts at the first one that isn't; a step
//! that finds part of its own output from before (one side's batch
//! transcript) reuses it. Once every step is done the meeting is `Done`.
//!
//! - **Transcribing** ([`Transcribing`]): the recording's two sides go to the
//!   Transcript job's batch STT in parallel, Them diarized and Me not; each
//!   answer is kept as `stt_them.json` / `stt_me.json` as it comes. They merge
//!   into turns without echoes ([`recap_engine::refine`]), saved as the
//!   refined transcript, and the meeting's speakers become `me`, `S1..Sn`.
//! - **Naming speakers** ([`crate::naming`]) and **Writing summary**
//!   ([`crate::summary`]) with the Summary and names job's AI
//!   ([`Processing::full`]). [`Processing::standard`] leaves them as [`Later`]
//!   slots (done at once); meetings processed that way, before the AI steps
//!   existed, get them with Regenerate ([`redo`]).

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use recap_engine::{
    BatchRequest, BatchStt, BatchTranscript, BoxFuture, Llm, ProviderError, Recording, SAMPLE_RATE, Turn,
    mic_levels::MicLevels, refine,
};
use recap_storage::{MeetingId, MeetingInfo, Segment, Speaker, Status, Store};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// Where the queue keeps its progress, in the meeting's folder.
pub const PROGRESS: &str = "processing.json";
/// The batch transcripts of the two sides, as the provider gave them.
pub const STT_THEM: &str = "stt_them.json";
pub const STT_ME: &str = "stt_me.json";
/// The recording, in the meeting's folder (the live meeting writes it).
pub const AUDIO: &str = "audio.ogg";

/// Below this level a side is silence (no mic): nothing to transcribe.
const SILENT: u16 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepId {
    Transcribing,
    NamingSpeakers,
    WritingSummary,
}

impl StepId {
    /// The steps, in the order they run.
    pub const ALL: [StepId; 3] = [StepId::Transcribing, StepId::NamingSpeakers, StepId::WritingSummary];

    pub fn as_str(self) -> &'static str {
        match self {
            StepId::Transcribing => "transcribing",
            StepId::NamingSpeakers => "naming_speakers",
            StepId::WritingSummary => "writing_summary",
        }
    }

    /// What the processing card says.
    pub fn label(self) -> &'static str {
        match self {
            StepId::Transcribing => "Transcribing",
            StepId::NamingSpeakers => "Naming speakers",
            StepId::WritingSummary => "Writing summary",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepState {
    pub id: StepId,
    pub status: StepStatus,
    /// How long it took (done), ms.
    #[serde(default)]
    pub took_ms: u64,
    /// Why it failed (error), for the user.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// Where a meeting's processing is: one state per step, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub steps: Vec<StepState>,
}

impl Default for Progress {
    fn default() -> Self {
        Self::of(&StepId::ALL)
    }
}

impl Progress {
    /// `ids`, all pending.
    fn of(ids: &[StepId]) -> Self {
        Self {
            steps: ids
                .iter()
                .map(|&id| StepState { id, status: StepStatus::Pending, took_ms: 0, error: String::new() })
                .collect(),
        }
    }

    /// The progress kept in meeting folder `folder`; all pending when there
    /// is none (never processed) or it can't be read.
    pub fn read(folder: &Path) -> Self {
        read_json(&folder.join(PROGRESS)).unwrap_or_default()
    }

    fn write(&self, folder: &Path) -> std::io::Result<()> {
        write_json(&folder.join(PROGRESS), self)
    }

    /// Every step done.
    pub fn finished(&self) -> bool {
        self.steps.iter().all(|s| s.status == StepStatus::Done)
    }

    /// The step that failed, if one did.
    pub fn failed(&self) -> Option<&StepState> {
        self.steps.iter().find(|s| s.status == StepStatus::Error)
    }

    pub fn step(&self, id: StepId) -> Option<&StepState> {
        self.steps.iter().find(|s| s.id == id)
    }

    /// The UI's `processing` value for meeting `id` (`ui/host.luau` has the
    /// shape).
    pub fn view(&self, id: &MeetingId) -> serde_json::Value {
        serde_json::json!({
            "id": id.as_str(),
            "steps": self.steps.iter().map(|s| serde_json::json!({
                "id": s.id.as_str(),
                "label": s.id.label(),
                "status": s.status,
                "took_ms": s.took_ms,
                "error": s.error,
            })).collect::<Vec<_>>(),
        })
    }
}

/// What processing needs from the user's settings.
#[derive(Clone, Debug, Default)]
pub struct ProcessingConfig {
    /// For the batch STT (glossary + attendees).
    pub keyterms: Vec<String>,
    /// Settings' "my name": who Me is (the naming step).
    pub my_name: String,
    /// Settings' Language, for the batch STT (`recap_engine::language_code`).
    pub language: Option<String>,
}

/// What a step works on.
#[derive(Clone)]
pub struct Job {
    pub store: Store,
    pub id: MeetingId,
    /// The meeting's folder.
    pub folder: PathBuf,
    pub config: ProcessingConfig,
}

/// A step of processing. It keeps its output in the meeting's folder or the
/// store before it returns Ok, so a later run can start after it.
pub trait Step: Send + Sync {
    fn id(&self) -> StepId;
    /// Do it for `job`'s meeting. The error is shown to the user as is.
    fn run(&self, job: Job) -> BoxFuture<'static, Result<(), String>>;
}

/// What processing tells whoever shows it.
#[derive(Clone, Debug)]
pub enum ProcessingEvent {
    /// Where it is now (after every change).
    Progress(Progress),
    /// A step finished: what it wrote (the transcript, the speakers, the
    /// summary) can be shown.
    StepDone(StepId),
    /// Every step is done: the meeting is `Done`. The last event.
    Finished(MeetingInfo),
    /// It stopped at a failed step (its error is in the last `Progress`), or
    /// could not run at all. The last event.
    Stopped(String),
    /// Something the user should see (a job's backup took over: "ElevenLabs
    /// failed (…); switched to Deepgram"). See [`with_notices`].
    Notice(String),
}

/// `events`, with each of `notices` (a job's `on_switch`) put in as a
/// `Notice`, ahead of the event that follows it (so before `Finished`).
/// Call it inside the runtime.
pub fn with_notices(
    mut events: UnboundedReceiver<ProcessingEvent>,
    mut notices: UnboundedReceiver<String>,
) -> UnboundedReceiver<ProcessingEvent> {
    let (out, merged) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                Some(text) = notices.recv() => drop(out.send(ProcessingEvent::Notice(text))),
                event = events.recv() => {
                    let Some(event) = event else { break };
                    while let Ok(text) = notices.try_recv() {
                        let _ = out.send(ProcessingEvent::Notice(text));
                    }
                    let _ = out.send(event);
                }
            }
        }
    });
    merged
}

/// The queue: its steps in order.
#[derive(Clone)]
pub struct Processing {
    steps: Vec<Arc<dyn Step>>,
}

impl Processing {
    /// Transcribing with `batch` (the Transcript job; an error when it has no
    /// provider), then the slots of the later steps.
    pub fn standard(batch: Result<Arc<dyn BatchStt>, ProviderError>) -> Self {
        Self {
            steps: vec![
                Arc::new(Transcribing::new(batch)),
                Arc::new(Later(StepId::NamingSpeakers)),
                Arc::new(Later(StepId::WritingSummary)),
            ],
        }
    }

    /// Every step: Transcribing with `batch` (the Transcript job), Naming
    /// speakers and Writing summary with `llm` (the Summary and names job);
    /// a job without a provider fails its step with the reason.
    pub fn full(batch: Result<Arc<dyn BatchStt>, ProviderError>, llm: Result<Arc<dyn Llm>, ProviderError>) -> Self {
        Self::standard(batch)
            .with(Arc::new(crate::naming::NamingSpeakers::new(llm.clone())))
            .with(Arc::new(crate::summary::WritingSummary::new(llm)))
    }

    /// `step` in the place of the step with its id.
    pub fn with(mut self, step: Arc<dyn Step>) -> Self {
        if let Some(slot) = self.steps.iter_mut().find(|s| s.id() == step.id()) {
            *slot = step;
        }
        self
    }

    /// Process meeting `id` from where it is, on the current tokio runtime
    /// (call it inside it).
    pub fn start(&self, store: Store, id: MeetingId, config: ProcessingConfig) -> UnboundedReceiver<ProcessingEvent> {
        let (out, events) = mpsc::unbounded_channel();
        let this = self.clone();
        tokio::spawn(async move { this.run(store, id, config, out).await });
        events
    }

    /// Process meeting `id` from where it is, telling `out`.
    pub async fn run(
        &self,
        store: Store,
        id: MeetingId,
        config: ProcessingConfig,
        out: UnboundedSender<ProcessingEvent>,
    ) {
        let send = |event: ProcessingEvent| drop(out.send(event));
        let folder = store.folder(&id);
        // What ran or failed before goes again; what is done stays done.
        let saved = Progress::read(&folder);
        let mut progress = Progress::of(&self.steps.iter().map(|s| s.id()).collect::<Vec<_>>());
        for step in &mut progress.steps {
            if let Some(done) = saved.step(step.id).filter(|s| s.status == StepStatus::Done) {
                *step = done.clone();
            }
        }
        // A meeting cut off while recording is processed as far as it got.
        if let Err(error) = store.update(&id, |info| {
            if info.status == Status::Recording {
                info.status = Status::Processing;
            }
        }) {
            send(ProcessingEvent::Stopped(error.to_string()));
            return;
        }
        let save = |progress: &Progress| {
            if let Err(error) = progress.write(&folder) {
                eprintln!("recap: {}: {error}", folder.join(PROGRESS).display());
            }
            send(ProcessingEvent::Progress(progress.clone()));
        };
        save(&progress);
        let job = Job { store: store.clone(), id: id.clone(), folder: folder.clone(), config };
        for (i, step) in self.steps.iter().enumerate() {
            if progress.steps[i].status == StepStatus::Done {
                continue;
            }
            progress.steps[i].status = StepStatus::Running;
            progress.steps[i].error.clear();
            save(&progress);
            let began = Instant::now();
            match step.run(job.clone()).await {
                Ok(()) => {
                    progress.steps[i].status = StepStatus::Done;
                    progress.steps[i].took_ms = began.elapsed().as_millis() as u64;
                    save(&progress);
                    send(ProcessingEvent::StepDone(step.id()));
                }
                Err(error) => {
                    progress.steps[i].status = StepStatus::Error;
                    progress.steps[i].error = error.clone();
                    save(&progress);
                    send(ProcessingEvent::Stopped(error));
                    return;
                }
            }
        }
        match store.update(&id, |info| info.status = Status::Done) {
            Ok(info) => send(ProcessingEvent::Finished(info)),
            Err(error) => send(ProcessingEvent::Stopped(error.to_string())),
        }
    }
}

/// Regenerate: `steps` of the meeting in `folder` run again the next time
/// it is processed (they are pending in its progress); the others keep
/// what they are.
pub fn redo(folder: &Path, steps: &[StepId]) -> std::io::Result<()> {
    let mut progress = Progress::read(folder);
    for state in &mut progress.steps {
        if steps.contains(&state.id) {
            *state = StepState { id: state.id, status: StepStatus::Pending, took_ms: 0, error: String::new() };
        }
    }
    progress.write(folder)
}

/// An empty slot: done at once, nothing written (`Processing::standard`).
pub struct Later(pub StepId);

impl Step for Later {
    fn id(&self) -> StepId {
        self.0
    }

    fn run(&self, _job: Job) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

/// Step 1: batch STT ×2 → turns → echo dedup → the refined transcript.
pub struct Transcribing {
    batch: Result<Arc<dyn BatchStt>, ProviderError>,
}

impl Transcribing {
    pub fn new(batch: Result<Arc<dyn BatchStt>, ProviderError>) -> Self {
        Self { batch }
    }
}

impl Step for Transcribing {
    fn id(&self) -> StepId {
        StepId::Transcribing
    }

    fn run(&self, job: Job) -> BoxFuture<'static, Result<(), String>> {
        let batch = self.batch.clone();
        Box::pin(async move { transcribe(batch, job).await })
    }
}

async fn maybe<T>(todo: Option<impl Future<Output = Result<T, String>>>) -> Result<Option<T>, String> {
    match todo {
        Some(todo) => todo.await.map(Some),
        None => Ok(None),
    }
}

async fn transcribe(batch: Result<Arc<dyn BatchStt>, ProviderError>, job: Job) -> Result<(), String> {
    let them_path = job.folder.join(STT_THEM);
    let me_path = job.folder.join(STT_ME);
    let mut them: Option<BatchTranscript> = read_json(&them_path);
    let mut me: Option<BatchTranscript> = read_json(&me_path);
    let mut recorded_ms = None;
    let audio = job.folder.join(AUDIO);
    let needs_stt = them.is_none() || me.is_none();
    if needs_stt && !audio.exists() {
        return Err("This meeting has no recording.".into());
    }
    // The recording: the batch's input when a side is missing, and its
    // levels judge echoes either way (ADR 0021).
    let recording = if audio.exists() {
        let read = tokio::task::spawn_blocking(move || Recording::read(&audio)).await.map_err(|e| e.to_string())?;
        match read {
            Ok(recording) => Some(recording),
            Err(e) if needs_stt => return Err(format!("The recording can't be read: {e}")),
            Err(_) => None,
        }
    } else {
        None
    };
    let levels = recording.as_ref().map(|r| MicLevels::from_recording(&r.me, &r.them));
    if needs_stt && let Some(recording) = recording {
        let batch = batch.map_err(|e| e.message)?;
        recorded_ms = Some(recording.them.len().max(recording.me.len()) as u64 * 1000 / u64::from(SAMPLE_RATE));
        let side = |samples: Vec<i16>, diarize: bool, path: PathBuf| {
            let (batch, keyterms, language) = (batch.clone(), job.config.keyterms.clone(), job.config.language.clone());
            async move {
                let heard = if samples.iter().all(|s| s.unsigned_abs() < SILENT) {
                    BatchTranscript::default()
                } else {
                    let request = BatchRequest { samples, diarize, keyterms, language };
                    batch.transcribe(request).await.map_err(|e| e.message)?
                };
                // Kept at once: a later run asks only for the side missing.
                write_json(&path, &heard).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok::<_, String>(heard)
            }
        };
        let (them_heard, me_heard) = tokio::join!(
            maybe(them.is_none().then(|| side(recording.them, true, them_path.clone()))),
            maybe(me.is_none().then(|| side(recording.me, false, me_path.clone()))),
        );
        them = them.or(them_heard?);
        me = me.or(me_heard?);
    }
    let (them, me) = (them.unwrap_or_default(), me.unwrap_or_default());
    let turns = refine::turns_with(&me, &them, levels.as_ref());
    let segments: Vec<Segment> = turns.iter().map(stored).collect();
    job.store.set_refined(&job.id, &segments).map_err(|e| e.to_string())?;
    job.store
        .update(&job.id, |info| {
            info.speakers = speakers(&turns, &info.speakers);
            if info.duration_ms == 0
                && let Some(ms) = recorded_ms
            {
                info.duration_ms = ms;
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn stored(turn: &Turn) -> Segment {
    Segment {
        source: turn.source,
        speaker: turn.speaker.clone(),
        start_ms: turn.start_ms,
        end_ms: turn.end_ms,
        text: turn.text.clone(),
    }
}

/// The speakers of `turns`: Me first, then `S1..Sn` in order; names given
/// before (a rerun) stay.
fn speakers(turns: &[Turn], before: &[Speaker]) -> Vec<Speaker> {
    let mut ids: Vec<&str> = Vec::new();
    if turns.iter().any(|t| t.speaker.as_deref() == Some(refine::ME)) {
        ids.push(refine::ME);
    }
    let mut voices: Vec<&str> =
        turns.iter().filter_map(|t| t.speaker.as_deref()).filter(|s| *s != refine::ME).collect();
    voices.sort_by_key(|s| (s.trim_start_matches('S').parse::<u32>().unwrap_or(u32::MAX), s.to_string()));
    voices.dedup();
    ids.extend(voices);
    ids.into_iter()
        .map(|id| Speaker { id: id.to_owned(), name: before.iter().find(|s| s.id == id).and_then(|s| s.name.clone()) })
        .collect()
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// Whole or not at all: a temporary file, then a rename over it.
pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
