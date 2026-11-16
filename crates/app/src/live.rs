//! How Start begins a meeting: its audio and its live captions provider
//! (what it reads from Settings comes from SettingsHost, in main.rs).
//!
//! By default: live capture (mic + meeting audio) and the user's Live
//! captions job (main with backup). Dev flags for tests and demos:
//!
//! - `--replay <file.wav>`: the WAV, in real time, instead of capture. With
//!   `ELEVENLABS_API_KEY` in the environment, ElevenLabs uses that key
//!   (no Credential Manager needed for a demo).
//! - `--scripted`: the test script (testdata/meeting_tts_script.txt) instead
//!   of a provider; without `--replay`, over silence.

use std::{path::PathBuf, sync::Arc};

use recap_engine::{
    AudioFrame, AudioSource, BoxFuture, Capabilities, LiveEvent, LiveOptions, LiveStt, ProviderError, Script,
    ScriptedLiveStt, SilenceSource, Source, WavSource,
};
use recap_providers::{Providers, elevenlabs};
use tokio::sync::mpsc;

use crate::presenter::LiveChoice;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Audio {
    /// The mic and the meeting audio.
    #[default]
    Capture,
    /// A WAV file, paced (stereo: Me, Them; mono: Them).
    Replay(PathBuf),
    /// Silence, paced (with `--scripted`: the test meeting without audio).
    Silence,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartOptions {
    pub audio: Audio,
    /// The scripted provider instead of the Live captions job.
    pub scripted: bool,
}

impl StartOptions {
    /// From the command line (`--replay <wav>`, `--scripted`).
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Self {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--replay" => {
                    if let Some(path) = args.next() {
                        options.audio = Audio::Replay(PathBuf::from(path));
                    }
                }
                "--scripted" => options.scripted = true,
                _ => {}
            }
        }
        if options.scripted && options.audio == Audio::Capture {
            options.audio = Audio::Silence;
        }
        options
    }

    /// The meeting's audio.
    pub fn source(&self) -> Result<Box<dyn AudioSource>, String> {
        match &self.audio {
            Audio::Replay(path) => WavSource::open(path)
                .map(|wav| Box::new(wav.paced()) as Box<dyn AudioSource>)
                .map_err(|error| format!("{}: {error}", path.display())),
            Audio::Silence => Ok(Box::new(SilenceSource::paced(None))),
            #[cfg(windows)]
            Audio::Capture => recap_engine::LiveCapture::start()
                .map(|capture| Box::new(capture) as Box<dyn AudioSource>)
                .map_err(|error| error.to_string()),
            #[cfg(not(windows))]
            Audio::Capture => Err("live capture needs Windows".into()),
        }
    }

    /// The live captions provider. When the job can't be built (no key, no
    /// provider) the meeting still records: `notify` says why, and captions
    /// stay empty.
    pub fn live(&self, providers: &Providers, notify: impl Fn(String) + Clone + Send + Sync + 'static) -> Arc<dyn LiveStt> {
        if self.scripted {
            return Arc::new(ScriptedLiveStt::new(Script::test_meeting()));
        }
        if matches!(self.audio, Audio::Replay(_))
            && let Some(key) = std::env::var("ELEVENLABS_API_KEY").ok().filter(|k| !k.trim().is_empty())
        {
            return Arc::new(elevenlabs::ElevenLabs::new("ElevenLabs", key.trim(), elevenlabs::Config::default()));
        }
        let on_switch = {
            let notify = notify.clone();
            move |n: recap_engine::FallbackNotice| notify(format!("{} failed ({}); switched to {}", n.from, n.error.message, n.to))
        };
        match providers.live(on_switch) {
            Ok(job) => Arc::new(job),
            Err(error) => {
                notify(format!("Live captions are off: {}", error.message));
                let name = providers
                    .choice(recap_providers::Job::Live)
                    .0
                    .and_then(|id| providers.settings(&id))
                    .map_or_else(|| "Live captions".to_owned(), |s| s.name);
                Arc::new(Unavailable { name, error })
            }
        }
    }

    /// The speech engines live captions could switch to, in the order
    /// Settings lists them.
    pub fn live_choices(providers: &Providers) -> Vec<LiveChoice> {
        providers
            .eligible(recap_providers::Job::Live)
            .into_iter()
            .filter_map(|id| providers.settings(&id).map(|s| LiveChoice { id, name: s.name }))
            .collect()
    }
}

/// No live captions: the job could not be built (named after its main).
struct Unavailable {
    name: String,
    error: ProviderError,
}

impl LiveStt for Unavailable {
    fn name(&self) -> &str {
        &self.name
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn run(
        &self,
        _source: Source,
        _options: LiveOptions,
        _audio: mpsc::UnboundedReceiver<AudioFrame>,
        _events: mpsc::UnboundedSender<LiveEvent>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let error = self.error.clone();
        Box::pin(async move { Err(error) })
    }
}
