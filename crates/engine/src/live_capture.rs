//! Live capture on Windows: the mic (Me) and the meeting audio (Them) through
//! WASAPI, each on a thread of its own, stamped with QPC time and put on one
//! [`Timeline`].
//!
//! - Me: the default communications capture device. When the default changes
//!   (headphones ↔ speakers) or the device goes away, the stream is reopened
//!   on the new default; the hole in between is zero-filled.
//! - Them: process loopback of everything except this process tree, so our
//!   own playback is never captioned. It is not tied to an output device, so
//!   switching outputs needs nothing; it is reopened only if it fails.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use wasapi::{
    AudioCaptureClient, AudioClient, DeviceEnumerator, Direction, Handle, Role, SampleType, StreamMode,
    WaveFormat,
};

use crate::{AudioFrame, AudioSource, Packet, SAMPLE_RATE, Source, Timeline};

/// How long one side may stall before it is zero-filled so the other keeps
/// flowing: well past WASAPI's packet interval (10 ms) and buffer (100 ms).
const WAIT: Duration = Duration::from_millis(300);
/// Between attempts to reopen a lost device.
const RETRY: Duration = Duration::from_millis(500);

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("the microphone could not be opened: {0}")]
    Mic(String),
    #[error("the meeting audio could not be captured: {0}")]
    MeetingAudio(String),
}

/// The active capture devices as `(id, friendly name)`, for Settings to
/// offer; empty when they can't be listed.
pub fn microphones() -> Vec<(String, String)> {
    // On a thread of its own: COM there is ours to set up (the UI thread's
    // is the window system's).
    std::thread::spawn(|| {
        let _com = wasapi::initialize_mta();
        let Ok(devices) = DeviceEnumerator::new().and_then(|e| e.get_device_collection(Direction::Capture)) else {
            return Vec::new();
        };
        let count = devices.get_nbr_devices().unwrap_or(0);
        (0..count)
            .filter_map(|i| devices.get_device_at_index(i).ok())
            .filter_map(|d| Some((d.get_id().ok()?, d.get_friendlyname().ok()?)))
            .collect()
    })
    .join()
    .unwrap_or_default()
}

/// The live [`AudioSource`]: mic and meeting audio, aligned. Dropping it stops
/// capture.
pub struct LiveCapture {
    packets: mpsc::Receiver<(Source, Packet)>,
    timeline: Timeline,
    queue: VecDeque<AudioFrame>,
    clock: Clock,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl LiveCapture {
    /// Open both streams; an error if either can't be opened at all. Once
    /// started, a lost device is reopened for as long as capture runs.
    pub fn start() -> Result<LiveCapture, CaptureError> {
        let clock = Clock::new();
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, packets) = mpsc::channel();
        let mut threads = Vec::new();

        let mic_changed = Arc::new(AtomicBool::new(false));
        let (mic_ready, mic_first) = mpsc::channel();
        threads.push(spawn("recap-capture-mic", {
            let (tx, stop, changed) = (tx.clone(), stop.clone(), mic_changed.clone());
            move || {
                let _com = wasapi::initialize_mta();
                // Default-device changes flag a reopen; kept alive while capturing.
                let _events = watch_default_mic(changed.clone());
                capture_loop(Source::Me, || WasapiStream::mic(clock), &tx, &stop, &changed, Some(mic_ready));
            }
        }));
        let (system_ready, system_first) = mpsc::channel();
        threads.push(spawn("recap-capture-system", {
            let (tx, stop) = (tx, stop.clone());
            move || {
                let _com = wasapi::initialize_mta();
                let never = AtomicBool::new(false);
                capture_loop(Source::Them, || WasapiStream::loopback(clock), &tx, &stop, &never, Some(system_ready));
            }
        }));

        let capture = LiveCapture { packets, timeline: Timeline::new(), queue: VecDeque::new(), clock, stop, threads };
        let first = |rx: mpsc::Receiver<Result<(), String>>| {
            rx.recv().unwrap_or_else(|_| Err("the capture thread stopped".into()))
        };
        first(mic_first).map_err(CaptureError::Mic)?;
        first(system_first).map_err(CaptureError::MeetingAudio)?;
        Ok(capture)
    }
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> JoinHandle<()> {
    std::thread::Builder::new().name(name.into()).spawn(f).expect("a capture thread starts")
}

impl AudioSource for LiveCapture {
    fn next_frame(&mut self) -> Option<AudioFrame> {
        loop {
            if let Some(frame) = self.queue.pop_front() {
                return Some(frame);
            }
            match self.packets.recv_timeout(Duration::from_millis(50)) {
                Ok((source, packet)) => {
                    self.timeline.push(source, packet);
                    while let Ok((source, packet)) = self.packets.try_recv() {
                        self.timeline.push(source, packet);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.queue.extend(self.timeline.finish());
                    return self.queue.pop_front();
                }
            }
            self.queue.extend(self.timeline.frames(self.clock.now(), WAIT));
        }
    }
}

impl Drop for LiveCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// Time since capture started, from the QPC: the clock WASAPI stamps packets
/// with, in 100 ns units.
#[derive(Clone, Copy)]
struct Clock {
    start_hns: u64,
}

impl Clock {
    fn new() -> Clock {
        Clock { start_hns: qpc_hns() }
    }

    fn now(&self) -> Duration {
        self.since(qpc_hns())
    }

    fn since(&self, hns: u64) -> Duration {
        Duration::from_nanos(hns.saturating_sub(self.start_hns) * 100)
    }
}

fn qpc_hns() -> u64 {
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    let (mut count, mut freq) = (0i64, 0i64);
    // SAFETY: plain out-parameters; both calls always succeed on XP and later.
    unsafe {
        let _ = QueryPerformanceCounter(&mut count);
        let _ = QueryPerformanceFrequency(&mut freq);
    }
    (count as i128 * 10_000_000 / freq.max(1) as i128) as u64
}

/// A capture stream: packets while it lives, an error once it is lost.
trait Stream {
    /// Wait for audio (briefly) and add what came; `Err` once the device is gone.
    fn read(&mut self, out: &mut Vec<Packet>) -> Result<(), String>;
}

/// Open, read until lost, reopen, until `stop`. `changed` asks for a reopen
/// (the default device moved). The first open's result goes to `first`; if
/// it fails, the loop ends there.
fn capture_loop<S: Stream>(
    source: Source,
    mut open: impl FnMut() -> Result<S, String>,
    tx: &mpsc::Sender<(Source, Packet)>,
    stop: &AtomicBool,
    changed: &AtomicBool,
    mut first: Option<mpsc::Sender<Result<(), String>>>,
) {
    let mut packets = Vec::new();
    while !stop.load(Ordering::SeqCst) {
        changed.store(false, Ordering::SeqCst);
        let mut stream = match open() {
            Ok(stream) => {
                if let Some(first) = first.take() {
                    let _ = first.send(Ok(()));
                }
                stream
            }
            Err(error) => {
                if let Some(first) = first.take() {
                    let _ = first.send(Err(error));
                    return;
                }
                std::thread::sleep(RETRY);
                continue;
            }
        };
        while !stop.load(Ordering::SeqCst) && !changed.load(Ordering::SeqCst) {
            if stream.read(&mut packets).is_err() {
                break; // lost: reopen
            }
            for packet in packets.drain(..) {
                if tx.send((source, packet)).is_err() {
                    return; // capture was dropped
                }
            }
        }
    }
}

/// A shared-mode WASAPI capture stream converted by Windows to 16 kHz mono
/// float.
struct WasapiStream {
    // The client must outlive the capture client and stays open with it.
    client: AudioClient,
    capture: AudioCaptureClient,
    event: Handle,
    clock: Clock,
    block_align: usize,
    bytes: Vec<u8>,
}

impl WasapiStream {
    fn format() -> WaveFormat {
        WaveFormat::new(32, 32, SampleType::Float, SAMPLE_RATE as usize, 1, None)
    }

    fn mic(clock: Clock) -> Result<WasapiStream, String> {
        let device = DeviceEnumerator::new()
            .and_then(|e| e.get_default_device_for_role(Direction::Capture, Role::Communications))
            .map_err(|e| e.to_string())?;
        let client = device.get_iaudioclient().map_err(|e| e.to_string())?;
        Self::start(client, clock, 1_000_000)
    }

    fn loopback(clock: Clock) -> Result<WasapiStream, String> {
        // Everything but this process tree (include_tree = false excludes it).
        let client = AudioClient::new_application_loopback_client(std::process::id(), false)
            .map_err(|e| e.to_string())?;
        // Process loopback can't report its buffer: 0 lets Windows choose.
        Self::start(client, clock, 0)
    }

    fn start(mut client: AudioClient, clock: Clock, buffer_hns: i64) -> Result<WasapiStream, String> {
        let format = Self::format();
        let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: buffer_hns };
        client.initialize_client(&format, Direction::Capture, mode, None).map_err(|e| e.to_string())?;
        let event = client.set_get_eventhandle().map_err(|e| e.to_string())?;
        let capture = client.get_audiocaptureclient().map_err(|e| e.to_string())?;
        client.start_stream().map_err(|e| e.to_string())?;
        Ok(WasapiStream { client, capture, event, clock, block_align: format.get_blockalign() as usize, bytes: Vec::new() })
    }
}

impl Stream for WasapiStream {
    fn read(&mut self, out: &mut Vec<Packet>) -> Result<(), String> {
        // A timeout is no packets (nothing playing): the timeline zero-fills.
        let _ = self.event.wait_for_event(200);
        loop {
            let frames = self.capture.get_next_packet_size().map_err(|e| e.to_string())?.unwrap_or(0);
            if frames == 0 {
                return Ok(());
            }
            self.bytes.resize(frames as usize * self.block_align, 0);
            let (read, info) = self.capture.read_from_device(&mut self.bytes).map_err(|e| e.to_string())?;
            if read == 0 {
                return Ok(());
            }
            let samples: Vec<f32> = self.bytes[..read as usize * self.block_align]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            let length = Duration::from_secs_f64(read as f64 / SAMPLE_RATE as f64);
            let at = if info.timestamp > 0 && !info.flags.timestamp_error {
                self.clock.since(info.timestamp)
            } else {
                // No device time: it was heard just now, all of it.
                self.clock.now().saturating_sub(length)
            };
            out.push(Packet { at, rate: SAMPLE_RATE, channels: 1, samples });
        }
    }
}

impl Drop for WasapiStream {
    fn drop(&mut self) {
        let _ = self.client.stop_stream();
    }
}

/// Raise `changed` when the default communications mic changes.
fn watch_default_mic(changed: Arc<AtomicBool>) -> Option<wasapi::DeviceEventRegistration> {
    let mut callbacks = wasapi::DeviceEventCallbacks::new();
    callbacks.set_default_device_callback(move |direction, role, _| {
        if direction == Direction::Capture && role == Role::Communications {
            changed.store(true, Ordering::SeqCst);
        }
    });
    DeviceEnumerator::new().ok()?.register_notification_callback(callbacks).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream that hands out 10 ms packets of `level` from `from_ms`, and is
    /// lost after `lose_after` of them.
    struct Fake {
        at_ms: u64,
        level: f32,
        left: usize,
    }

    impl Stream for Fake {
        fn read(&mut self, out: &mut Vec<Packet>) -> Result<(), String> {
            if self.left == 0 {
                return Err("AUDCLNT_E_DEVICE_INVALIDATED".into());
            }
            self.left -= 1;
            out.push(Packet {
                at: Duration::from_millis(self.at_ms),
                rate: SAMPLE_RATE,
                channels: 1,
                samples: vec![self.level; SAMPLE_RATE as usize / 100],
            });
            self.at_ms += 10;
            Ok(())
        }
    }

    fn collect(rx: mpsc::Receiver<(Source, Packet)>, them_until_ms: u64) -> Vec<AudioFrame> {
        let mut timeline = Timeline::new();
        for (source, packet) in rx {
            timeline.push(source, packet);
        }
        for at in (0..them_until_ms).step_by(10) {
            timeline.push(
                Source::Them,
                Packet { at: Duration::from_millis(at), rate: SAMPLE_RATE, channels: 1, samples: vec![0.5; 160] },
            );
        }
        let mut frames = timeline.frames(Duration::from_millis(them_until_ms), Duration::ZERO);
        frames.extend(timeline.finish());
        frames
    }

    fn me(frames: &[AudioFrame]) -> Vec<i16> {
        frames.iter().filter(|f| f.source == Source::Me).flat_map(|f| f.samples.clone()).collect()
    }

    #[test]
    fn a_lost_device_is_reopened_and_the_hole_is_zero_filled() {
        // The mic is lost after 1 s; the reopened one comes back 0.5 s later
        // (a new default device), then is stopped after another second.
        let (tx, rx) = mpsc::channel();
        let stop = AtomicBool::new(false);
        let changed = AtomicBool::new(false);
        let mut opens = 0;
        let (first_tx, first_rx) = mpsc::channel();
        capture_loop(
            Source::Me,
            || {
                opens += 1;
                match opens {
                    1 => Ok(Fake { at_ms: 0, level: 0.25, left: 100 }),
                    2 => Err("no default device yet".into()),
                    3 => Ok(Fake { at_ms: 1_500, level: 0.25, left: 100 }),
                    _ => {
                        stop.store(true, Ordering::SeqCst);
                        Err("stopped".into())
                    }
                }
            },
            &tx,
            &stop,
            &changed,
            Some(first_tx),
        );
        drop(tx);
        assert_eq!(first_rx.recv().unwrap(), Ok(()));
        assert_eq!(opens, 4, "reopened until it worked, then again after the second loss");

        let me = me(&collect(rx, 2_500));
        assert_eq!(me.len(), 2_500 * 16, "Me spans the whole meeting");
        let level = |from_ms: usize, to_ms: usize| {
            me[from_ms * 16..to_ms * 16].iter().map(|s| s.unsigned_abs() as f64).sum::<f64>() / ((to_ms - from_ms) * 16) as f64
        };
        assert!(level(100, 900) > 8_000.0, "before the loss");
        assert!(level(1_050, 1_450) < 1.0, "the hole is zeros");
        assert!(level(1_600, 2_400) > 8_000.0, "the new device at its own time");
    }

    #[test]
    fn a_failed_first_open_is_reported_and_not_retried() {
        let (tx, _rx) = mpsc::channel();
        let (first_tx, first_rx) = mpsc::channel();
        let mut opens = 0;
        capture_loop::<Fake>(
            Source::Them,
            || {
                opens += 1;
                Err("E_ACCESSDENIED".into())
            },
            &tx,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            Some(first_tx),
        );
        assert_eq!(first_rx.recv().unwrap(), Err("E_ACCESSDENIED".to_string()));
        assert_eq!(opens, 1);
    }

    #[test]
    fn a_default_device_change_reopens_the_stream() {
        let (tx, rx) = mpsc::channel();
        let stop = AtomicBool::new(false);
        let changed = AtomicBool::new(false);
        let mut opens = 0;
        capture_loop(
            Source::Me,
            || {
                opens += 1;
                if opens == 1 {
                    // The user switches to headphones while this one runs.
                    changed.store(true, Ordering::SeqCst);
                    Ok(Fake { at_ms: 0, level: 0.25, left: 1_000 })
                } else {
                    stop.store(true, Ordering::SeqCst);
                    Ok(Fake { at_ms: 0, level: 0.25, left: 0 })
                }
            },
            &tx,
            &stop,
            &changed,
            None,
        );
        drop(tx);
        assert_eq!(opens, 2, "the change reopened it");
        let _ = rx;
    }
}
