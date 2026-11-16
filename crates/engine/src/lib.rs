//! Recap's headless meeting engine.
//!
//! Events in, `MeetingResult` out: a [`Meeting`] reads an [`AudioSource`]
//! (Me = mic, Them = meeting audio), sends each side to a [`LiveStt`]
//! provider, and emits [`EngineEvent`]s (segments partial and final, state
//! changes, the elapsed time) ending with `Finished(MeetingResult)`.
//!
//! Audio comes from [`LiveCapture`] (WASAPI mic + process loopback, Windows)
//! or a [`WavSource`] (tests, replays); both go through one [`Timeline`] so
//! Me and Them stay aligned (ADR 0003). With `MeetingConfig::recording` set,
//! the meeting is written to a stereo Ogg/Opus file by a [`Recorder`];
//! [`Recording::read`] decodes one back.
//!
//! No UI and no storage live here. The app turns events into UI state; later
//! tickets add the recorder, storage and the after-Stop processing behind the
//! same seams ([`BatchStt`], [`Llm`]).
//!
//! [`fake`] holds the storage-free fakes: a [`SilenceSource`] and a
//! [`ScriptedLiveStt`] that replays `testdata/meeting_tts_script.txt` as
//! partial → final segments, clocked by the audio it is given.

mod audio;
mod event;
pub mod fake;
mod fallback;
#[cfg(windows)]
mod live_capture;
mod live_meeting;
mod provider;
mod recorder;
mod timeline;
mod transcript;
mod wav_source;

pub use audio::{AudioFrame, AudioSource, FeedSource, FrameFeed, SAMPLE_RATE, SilenceSource};
#[cfg(windows)]
pub use live_capture::{CaptureError, LiveCapture, microphones};
pub use recorder::{Recorder, Recording};
pub use timeline::{Packet, Timeline};
pub use wav_source::WavSource;
pub use event::{EngineEvent, MeetingResult, MeetingState, Segment, SegmentId, Source};
pub use fake::{Script, ScriptLine, ScriptedLiveStt};
pub use fallback::{Fallback, FallbackNotice};
pub use live_meeting::{Meeting, MeetingConfig, MeetingControl};
pub use provider::{
    BatchRequest, BatchStt, BatchTranscript, BoxFuture, Capabilities, LiveEvent, LiveOptions,
    LiveStt, Llm, ProviderError, ProviderErrorKind, Word,
};
pub use transcript::{LiveTranscript, Row, RowChange};
