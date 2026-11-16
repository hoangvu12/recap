//! How Start begins a meeting: its audio, its live captions provider and the
//! settings it reads.
//!
//! By default: live capture (mic + meeting audio) and the user's Live
//! captions job (main with backup). Dev flags for tests and demos:
//!
//! - `--replay <file.wav>`: the WAV, in real time, instead of capture. With
//!   `ELEVENLABS_API_KEY` in the environment, ElevenLabs uses that key
//!   (no Credential Manager needed for a demo).
//! - `--scripted`: the test script (testdata/meeting_tts_script.txt) instead
//!   of a provider; without `--replay`, over silence.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use recap_engine::{
    AudioFrame, AudioSource, BoxFuture, Capabilities, LiveEvent, LiveOptions, LiveStt, ProviderError, Script,
    ScriptedLiveStt, SilenceSource, Source, WavSource,
};
use recap_providers::{Providers, elevenlabs};
use tokio::sync::mpsc;

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
                Arc::new(Unavailable(error))
            }
        }
    }
}

/// No live captions: the job could not be built.
struct Unavailable(ProviderError);

impl LiveStt for Unavailable {
    fn name(&self) -> &str {
        "Unavailable"
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
        let error = self.0.clone();
        Box::pin(async move { Err(error) })
    }
}

/// What a live meeting reads from the user's settings (`settings.json` in
/// `home`, ticket #16's file). A minimal reader until `SettingsHost` lands
/// here: "Caption my own speech" and the glossary as keyterms.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveSettings {
    pub caption_my_speech: bool,
    pub live: LiveOptions,
}

impl LiveSettings {
    pub fn load(home: &Path) -> Self {
        let Some(json) = std::fs::read(home.join("settings.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        else {
            return Self::default();
        };
        let keyterms = json["glossary"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_str())
            .map(str::trim)
            .filter(|t| !t.is_empty() && t.chars().count() <= 20)
            .take(50)
            .map(String::from)
            .collect();
        Self {
            caption_my_speech: json["caption_my_speech"].as_bool().unwrap_or(false),
            live: LiveOptions { keyterms, language: None },
        }
    }
}
