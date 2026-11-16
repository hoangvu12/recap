use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::Source;

/// Every stream is 16 kHz mono 16-bit by the time it reaches the engine.
pub const SAMPLE_RATE: u32 = 16_000;

/// A block of audio from one side, on that side's timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioFrame {
    pub source: Source,
    /// Where it starts on its side's timeline, in milliseconds.
    pub start_ms: u64,
    pub samples: Vec<i16>,
}

impl AudioFrame {
    pub fn silence(source: Source, start_ms: u64, duration_ms: u64) -> Self {
        let len = (duration_ms * SAMPLE_RATE as u64 / 1000) as usize;
        Self { source, start_ms, samples: vec![0; len] }
    }

    pub fn duration_ms(&self) -> u64 {
        self.samples.len() as u64 * 1000 / SAMPLE_RATE as u64
    }
}

/// Where a meeting's audio comes from: live capture (mic + loopback), a WAV
/// file, or a fake. Pulled on a thread of its own, so it may block; `None`
/// ends the meeting's audio.
pub trait AudioSource: Send {
    fn next_frame(&mut self) -> Option<AudioFrame>;
}

const FRAME_MS: u64 = 100;

/// Silence on both sides for a while: what the scripted fake STT is clocked
/// by. Unpaced it hands out frames as fast as they are read (tests); paced it
/// keeps real time (the app on fake data).
pub struct SilenceSource {
    total_ms: Option<u64>,
    at_ms: u64,
    next: Source,
    paced: Option<Instant>,
}

impl SilenceSource {
    /// `total` of silence, as fast as it is read.
    pub fn new(total: Duration) -> Self {
        Self { total_ms: Some(total.as_millis() as u64), at_ms: 0, next: Source::Me, paced: None }
    }

    /// Silence in real time, for `total` or until the meeting stops.
    pub fn paced(total: Option<Duration>) -> Self {
        Self {
            total_ms: total.map(|t| t.as_millis() as u64),
            at_ms: 0,
            next: Source::Me,
            paced: Some(Instant::now()),
        }
    }
}

impl AudioSource for SilenceSource {
    fn next_frame(&mut self) -> Option<AudioFrame> {
        if self.total_ms.is_some_and(|total| self.at_ms >= total) {
            return None;
        }
        let source = self.next;
        let frame = AudioFrame::silence(source, self.at_ms, FRAME_MS);
        if source == Source::Them {
            if let Some(started) = self.paced {
                let due = started + Duration::from_millis(self.at_ms + FRAME_MS);
                if let Some(wait) = due.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
            self.at_ms += FRAME_MS;
            self.next = Source::Me;
        } else {
            self.next = Source::Them;
        }
        Some(frame)
    }
}

/// The sending end of a [`FeedSource`]: a test pushes audio when it wants.
/// Dropping it ends the source.
pub struct FrameFeed {
    tx: mpsc::Sender<AudioFrame>,
    at_ms: std::cell::Cell<u64>,
}

/// An [`AudioSource`] fed by hand through its [`FrameFeed`].
pub struct FeedSource {
    rx: mpsc::Receiver<AudioFrame>,
}

impl FrameFeed {
    pub fn new() -> (FrameFeed, FeedSource) {
        let (tx, rx) = mpsc::channel();
        (FrameFeed { tx, at_ms: std::cell::Cell::new(0) }, FeedSource { rx })
    }

    pub fn push(&self, frame: AudioFrame) {
        let _ = self.tx.send(frame);
    }

    /// `duration` of silence on both sides, in 100 ms frames.
    pub fn push_silence(&self, duration: Duration) {
        let end = self.at_ms.get() + duration.as_millis() as u64;
        while self.at_ms.get() < end {
            let at = self.at_ms.get();
            self.push(AudioFrame::silence(Source::Me, at, FRAME_MS));
            self.push(AudioFrame::silence(Source::Them, at, FRAME_MS));
            self.at_ms.set(at + FRAME_MS);
        }
    }
}

impl AudioSource for FeedSource {
    fn next_frame(&mut self) -> Option<AudioFrame> {
        self.rx.recv().ok()
    }
}
