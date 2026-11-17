//! A job's main provider with its backup (GLOSSARY: Job). [`Fallback`] is a
//! provider itself ([`LiveStt`], [`BatchStt`] or [`Llm`]): it asks the main,
//! and when the main fails on quota, auth or the network
//! ([`ProviderError::falls_back`]) it asks the backup, telling `on_switch`.
//! Any other error is the job's own and is returned as it is.
//!
//! Live captions switch mid-session: the audio the main had not captioned as
//! final yet (since its last final) is replayed to the backup, then the rest
//! of the meeting's audio goes to it.

use std::{collections::VecDeque, sync::Arc};

use tokio::sync::mpsc;

use crate::{
    AudioFrame, Source,
    provider::{
        BatchRequest, BatchStt, BatchTranscript, BoxFuture, Capabilities, LiveEvent, LiveOptions, LiveStatus, LiveStt,
        Llm, ProviderError,
    },
};

/// The main provider gave up on a job and its backup took it over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FallbackNotice {
    /// The main provider's name.
    pub from: String,
    /// The backup's name.
    pub to: String,
    /// Why the main gave up.
    pub error: ProviderError,
}

type OnSwitch = Arc<dyn Fn(FallbackNotice) + Send + Sync>;

/// A main provider and, maybe, its backup: one provider to the job.
pub struct Fallback<T: ?Sized> {
    main: Arc<T>,
    backup: Option<Arc<T>>,
    on_switch: Option<OnSwitch>,
}

impl<T: ?Sized> Clone for Fallback<T> {
    fn clone(&self) -> Self {
        Self { main: self.main.clone(), backup: self.backup.clone(), on_switch: self.on_switch.clone() }
    }
}

impl<T: ?Sized> Fallback<T> {
    pub fn new(main: Arc<T>, backup: Option<Arc<T>>) -> Self {
        Self { main, backup, on_switch: None }
    }

    /// Hear each switch to the backup (to tell the user, or log it).
    pub fn on_switch(mut self, notify: impl Fn(FallbackNotice) + Send + Sync + 'static) -> Self {
        self.on_switch = Some(Arc::new(notify));
        self
    }

    pub fn main(&self) -> &Arc<T> {
        &self.main
    }

    pub fn backup(&self) -> Option<&Arc<T>> {
        self.backup.as_ref()
    }
}

fn notify(on_switch: &Option<OnSwitch>, from: &str, to: &str, error: &ProviderError) {
    if let Some(notify) = on_switch {
        notify(FallbackNotice { from: from.into(), to: to.into(), error: error.clone() });
    }
}

impl Llm for Fallback<dyn Llm> {
    fn name(&self) -> &str {
        self.main.name()
    }

    fn complete_json(
        &self,
        prompt: String,
        schema: serde_json::Value,
    ) -> BoxFuture<'static, Result<serde_json::Value, ProviderError>> {
        let job = self.clone();
        Box::pin(async move {
            match job.main.complete_json(prompt.clone(), schema.clone()).await {
                Err(error) if error.falls_back() && job.backup.is_some() => {
                    let backup = job.backup.as_ref().expect("checked");
                    notify(&job.on_switch, job.main.name(), backup.name(), &error);
                    backup.complete_json(prompt, schema).await
                }
                answer => answer,
            }
        })
    }
}

impl BatchStt for Fallback<dyn BatchStt> {
    fn name(&self) -> &str {
        self.main.name()
    }

    fn capabilities(&self) -> Capabilities {
        self.main.capabilities()
    }

    fn transcribe(&self, request: BatchRequest) -> BoxFuture<'static, Result<BatchTranscript, ProviderError>> {
        let job = self.clone();
        Box::pin(async move {
            match job.main.transcribe(request.clone()).await {
                Err(error) if error.falls_back() && job.backup.is_some() => {
                    let backup = job.backup.as_ref().expect("checked");
                    notify(&job.on_switch, job.main.name(), backup.name(), &error);
                    backup.transcribe(request).await
                }
                answer => answer,
            }
        })
    }
}

/// At most this much audio waits for a final, to be replayed on a switch.
const REPLAY_FRAMES: usize = 300;

impl LiveStt for Fallback<dyn LiveStt> {
    fn name(&self) -> &str {
        self.main.name()
    }

    fn capabilities(&self) -> Capabilities {
        self.main.capabilities()
    }

    fn run(
        &self,
        source: Source,
        options: LiveOptions,
        audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let (status, _) = mpsc::unbounded_channel();
        self.run_reporting(source, options, audio, events, status)
    }

    /// Each provider's statuses are passed on, and a switch says which
    /// provider captions from then on.
    fn run_reporting(
        &self,
        source: Source,
        options: LiveOptions,
        mut audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
        status: mpsc::UnboundedSender<LiveStatus>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let chain: Vec<Arc<dyn LiveStt>> = std::iter::once(self.main.clone()).chain(self.backup.clone()).collect();
        let on_switch = self.on_switch.clone();
        Box::pin(async move {
            // The audio since the last final, oldest first.
            let mut unheard: VecDeque<AudioFrame> = VecDeque::new();
            let mut audio_open = true;
            let mut at = 0;
            loop {
                let provider = chain[at].clone();
                let (frames, provider_audio) = mpsc::unbounded_channel();
                let (provider_events, mut heard) = mpsc::unbounded_channel();
                for frame in &unheard {
                    let _ = frames.send(frame.clone());
                }
                let mut frames = audio_open.then_some(frames);
                let mut session =
                    provider.run_reporting(source, options.clone(), provider_audio, provider_events, status.clone());
                let forward = |event: LiveEvent, unheard: &mut VecDeque<AudioFrame>| {
                    if event.is_final {
                        while unheard.front().is_some_and(|f| f.start_ms + f.duration_ms() <= event.end_ms) {
                            unheard.pop_front();
                        }
                    }
                    let _ = events.send(event);
                };
                let result = loop {
                    tokio::select! {
                        biased;
                        Some(event) = heard.recv() => forward(event, &mut unheard),
                        result = &mut session => break result,
                        frame = audio.recv(), if audio_open => match frame {
                            Some(frame) => {
                                if unheard.len() == REPLAY_FRAMES {
                                    unheard.pop_front();
                                }
                                unheard.push_back(frame.clone());
                                if let Some(frames) = &frames {
                                    let _ = frames.send(frame);
                                }
                            }
                            None => {
                                audio_open = false;
                                frames = None;
                            }
                        },
                    }
                };
                drop(session);
                while let Ok(event) = heard.try_recv() {
                    forward(event, &mut unheard);
                }
                match result {
                    Err(error) if error.falls_back() && at + 1 < chain.len() => {
                        notify(&on_switch, provider.name(), chain[at + 1].name(), &error);
                        at += 1;
                        let _ = status.send(LiveStatus::Using(chain[at].name().to_owned()));
                    }
                    result => return result,
                }
            }
        })
    }
}
