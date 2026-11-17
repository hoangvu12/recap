//! What the live adapters share: why a connection ended, the map from what
//! a connection was sent back to the side's timeline, the audio a side keeps
//! for the next connection, and how long to wait before it.

use std::collections::VecDeque;
use std::time::Duration;

use futures::stream::SplitSink;
use recap_engine::{AudioFrame, ProviderError, ProviderErrorKind};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};
use tokio_tungstenite::tungstenite::Message;

pub(super) type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
pub(super) type Sink = SplitSink<Ws, Message>;

/// At most this much audio (60 s) waits for a final, to be replayed after a
/// reconnect.
const UNFINAL_FRAMES: usize = 600;
const MAX_BACKOFF: Duration = Duration::from_secs(8);

/// Why a connection ended early.
pub(super) enum Failure {
    /// Worth a reconnect: the network, a session limit, a busy server.
    Retry(ProviderError),
    /// The captions are over: the key, the credit, a request the server
    /// won't take.
    Fatal(ProviderError),
}

impl Failure {
    pub(super) fn error(self) -> ProviderError {
        match self {
            Failure::Retry(e) | Failure::Fatal(e) => e,
        }
    }
}

/// A network failure, worth a reconnect, told as `vendor: message`.
pub(super) fn network(vendor: &str, message: impl std::fmt::Display) -> Failure {
    Failure::Retry(ProviderError::new(ProviderErrorKind::Network, format!("{vendor}: {message}")))
}

/// Where a chunk sent on a connection sits on the side's timeline.
#[derive(Clone, Copy, Debug)]
struct Anchor {
    session_ms: u64,
    session_len: u64,
    side_ms: u64,
    /// 0 for keepalive silence: it is on no timeline.
    side_len: u64,
}

/// What one connection was sent. Its times are seconds of that audio (a
/// replay, keepalive silence); the anchors land them on the side's
/// timeline, the meeting's clock.
#[derive(Default)]
pub(super) struct Anchors {
    list: Vec<Anchor>,
    session_ms: u64,
}

impl Anchors {
    /// `session_len` ms more were sent: `side_len` ms of the side's audio
    /// from `side_ms` (0 for keepalive silence).
    pub(super) fn sent(&mut self, session_len: u64, side_ms: u64, side_len: u64) {
        self.list.push(Anchor { session_ms: self.session_ms, session_len, side_ms, side_len });
        self.session_ms += session_len;
    }

    /// Connection seconds on the side's timeline (`fallback` before any audio).
    pub(super) fn side_ms(&self, seconds: f64, fallback: u64) -> u64 {
        let at = (seconds.max(0.0) * 1000.0).round() as u64;
        let i = self.list.partition_point(|a| a.session_ms <= at);
        match i.checked_sub(1).and_then(|i| self.list.get(i)) {
            Some(a) => a.side_ms + (at - a.session_ms).min(a.side_len),
            None => self.list.first().map_or(fallback, |a| a.side_ms),
        }
    }

    /// Forget anchors wholly before connection time `seconds`.
    pub(super) fn forget_before(&mut self, seconds: f64) {
        let at = (seconds.max(0.0) * 1000.0) as u64;
        let keep = self.list.partition_point(|a| a.session_ms + a.session_len <= at);
        let keep = keep.min(self.list.len().saturating_sub(1));
        self.list.drain(..keep);
    }
}

/// A side's audio as its captions keep it, across connections.
pub(super) struct SideAudio {
    /// The audio since the last final, oldest first: replayed on reconnect.
    unfinal: VecDeque<AudioFrame>,
    /// Where the next caption starts: the last final's end (or the first audio).
    pub(super) heard_to: Option<u64>,
    /// The end of the latest audio.
    pub(super) latest_ms: u64,
    /// The audio hasn't closed yet.
    pub(super) open: bool,
}

impl SideAudio {
    pub(super) fn new() -> Self {
        Self { unfinal: VecDeque::new(), heard_to: None, latest_ms: 0, open: true }
    }

    /// A frame came: kept until a final covers it.
    pub(super) fn keep(&mut self, frame: &AudioFrame) {
        self.heard_to.get_or_insert(frame.start_ms);
        self.latest_ms = self.latest_ms.max(frame.start_ms + frame.duration_ms());
        if self.unfinal.len() == UNFINAL_FRAMES {
            self.unfinal.pop_front();
        }
        self.unfinal.push_back(frame.clone());
    }

    /// A final ends at `end_ms`: the audio it covers needs no replay, and
    /// the next caption starts after it.
    pub(super) fn said_to(&mut self, end_ms: u64) {
        while self.unfinal.front().is_some_and(|f| f.start_ms + f.duration_ms() <= end_ms) {
            self.unfinal.pop_front();
        }
        self.heard_to = Some(self.heard_to.map_or(end_ms, |h| h.max(end_ms)));
    }

    /// What a new connection is sent first: the audio no final covers yet.
    pub(super) fn replay(&self) -> Vec<AudioFrame> {
        self.unfinal.iter().cloned().collect()
    }

    /// Back off for `wait`, keeping the audio that comes meanwhile (or
    /// until the audio closes).
    pub(super) async fn back_off(&mut self, wait: Duration, audio: &mut mpsc::UnboundedReceiver<AudioFrame>) {
        let until = Instant::now() + wait;
        while self.open {
            tokio::select! {
                _ = sleep_until(until) => return,
                frame = audio.recv() => match frame {
                    Some(frame) => self.keep(&frame),
                    None => self.open = false,
                },
            }
        }
    }
}

/// Reconnecting: the failures in a row so far.
#[derive(Default)]
pub(super) struct Retries {
    failures: u32,
}

impl Retries {
    /// A connection failed with `error` (`heard`: it heard speech first, so
    /// the count starts over). The wait before the next one, doubling from
    /// `backoff` up to 8 s; the error once more than `retries` failed in a row.
    pub(super) fn after(
        &mut self,
        heard: bool,
        error: ProviderError,
        retries: u32,
        backoff: Duration,
    ) -> Result<Duration, ProviderError> {
        if heard {
            self.failures = 0;
        }
        self.failures += 1;
        if self.failures > retries {
            return Err(ProviderError::new(ProviderErrorKind::Network, error.message));
        }
        Ok(backoff.saturating_mul(1 << (self.failures - 1).min(16)).min(MAX_BACKOFF))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_time_maps_through_anchors() {
        let mut anchors = Anchors::default();
        anchors.sent(100, 5_000, 100);
        // keepalive silence: on no timeline
        anchors.sent(100, 5_100, 0);
        anchors.sent(100, 9_000, 100);
        assert_eq!(anchors.side_ms(0.05, 0), 5_050);
        assert_eq!(anchors.side_ms(0.15, 0), 5_100);
        assert_eq!(anchors.side_ms(0.25, 0), 9_050);
        assert_eq!(anchors.side_ms(9.0, 0), 9_100, "past the end clamps");
        assert_eq!(Anchors::default().side_ms(1.0, 42), 42, "before any audio");
        anchors.forget_before(0.25);
        assert_eq!(anchors.list.len(), 1);
    }

    #[test]
    fn reconnects_back_off_and_give_up() {
        let base = Duration::from_millis(500);
        let error = || ProviderError::new(ProviderErrorKind::Network, "down");
        let mut retries = Retries::default();
        let waits: Vec<u64> =
            (0..5).map(|_| retries.after(false, error(), 5, base).unwrap().as_millis() as u64).collect();
        assert_eq!(waits, [500, 1_000, 2_000, 4_000, 8_000]);
        assert!(retries.after(false, error(), 5, base).is_err(), "the sixth in a row");
        // A connection that heard speech starts the count over.
        assert_eq!(retries.after(true, error(), 5, base).unwrap(), base);
    }
}
