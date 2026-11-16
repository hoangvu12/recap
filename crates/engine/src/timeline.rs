//! One timeline for both sides: stamped packets in (any rate, any channel
//! count), 16 kHz mono frames out, Me and Them frame for frame.
//!
//! Each packet carries the time its first sample was heard, on a clock both
//! sides share (QPC for live capture, the file position for a WAV). Where a
//! side's packets leave a hole, it gets zeros; where they overlap what was
//! already heard, the overlap is dropped. A side that goes quiet (no packets)
//! is zero-filled once it is `wait` behind, so the other side keeps flowing.

use std::collections::VecDeque;
use std::time::Duration;

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::{AudioFrame, SAMPLE_RATE, Source};

/// Raw audio from one capture stream.
#[derive(Clone, Debug)]
pub struct Packet {
    /// When its first sample was heard, from the start of the meeting.
    pub at: Duration,
    pub rate: u32,
    pub channels: u16,
    /// Interleaved, -1.0..1.0.
    pub samples: Vec<f32>,
}

/// Frames are 100 ms: 1600 samples at 16 kHz.
const FRAME: u64 = SAMPLE_RATE as u64 / 10;
/// Clock jitter below this is not a gap (or an overlap).
const JITTER_S: f64 = 0.020;
/// A jump this large re-anchors the side instead of padding through it.
const RESYNC_S: f64 = 1.0;

/// Packets in, aligned frames out. See the module docs.
pub struct Timeline {
    me: Lane,
    them: Lane,
    /// Samples handed out per side so far.
    emitted: u64,
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Timeline {
    pub fn new() -> Self {
        Self { me: Lane::default(), them: Lane::default(), emitted: 0 }
    }

    pub fn push(&mut self, source: Source, packet: Packet) {
        self.lane(source).push(packet);
    }

    /// The frames ready at `now` (on the packets' clock): those both sides
    /// have heard, and, zero-filled, those a side is more than `wait` late for.
    pub fn frames(&mut self, now: Duration, wait: Duration) -> Vec<AudioFrame> {
        let horizon = to_samples(now.saturating_sub(wait).as_secs_f64());
        self.emit(|end, me, them| (me >= end && them >= end) || end <= horizon)
    }

    /// The rest, once no more packets come: up to the end of the longer side.
    pub fn finish(&mut self) -> Vec<AudioFrame> {
        let me = self.me.finish();
        let them = self.them.finish();
        let end = me.max(them);
        let mut frames = self.emit(|frame_end, _, _| frame_end <= end);
        if self.emitted < end {
            // The last frame, short: just what is left.
            let len = (end - self.emitted) as usize;
            frames.extend(self.take_frame(len));
        }
        frames
    }

    fn lane(&mut self, source: Source) -> &mut Lane {
        match source {
            Source::Me => &mut self.me,
            Source::Them => &mut self.them,
        }
    }

    fn emit(&mut self, ready: impl Fn(u64, u64, u64) -> bool) -> Vec<AudioFrame> {
        let mut frames = Vec::new();
        while ready(self.emitted + FRAME, self.me.produced, self.them.produced) {
            frames.extend(self.take_frame(FRAME as usize));
        }
        frames
    }

    fn take_frame(&mut self, len: usize) -> [AudioFrame; 2] {
        let start_ms = self.emitted * 1000 / SAMPLE_RATE as u64;
        let me = AudioFrame { source: Source::Me, start_ms, samples: self.me.take(len) };
        let them = AudioFrame { source: Source::Them, start_ms, samples: self.them.take(len) };
        self.emitted += len as u64;
        [me, them]
    }
}

fn to_samples(seconds: f64) -> u64 {
    (seconds.max(0.0) * SAMPLE_RATE as f64).round() as u64
}

/// One side.
#[derive(Default)]
struct Lane {
    rate: u32,
    resampler: Option<To16k>,
    /// Where the next input sample belongs, in seconds; `None` until anchored.
    next_at: Option<f64>,
    /// The timeline position after the last sample produced (or skipped).
    produced: u64,
    /// Produced, not yet handed out: the samples from the timeline's
    /// `emitted` up to `produced`.
    out: VecDeque<i16>,
    /// Samples handed out (the timeline's own count): what this side
    /// produces before that was already handed out zero-filled, and is dropped.
    emitted: u64,
}

impl Lane {
    fn push(&mut self, packet: Packet) {
        if packet.rate == 0 || packet.channels == 0 {
            return;
        }
        if packet.rate != self.rate {
            self.rate = packet.rate;
            self.resampler = None;
            self.next_at = None;
        }
        let mono = downmix(&packet.samples, packet.channels as usize);
        let rate = self.rate as f64;
        let at = packet.at.as_secs_f64();
        let mut input: &[f32] = &mono;
        let mut cursor = match self.next_at {
            Some(next) if (at - next).abs() < RESYNC_S => {
                let drift = at - next;
                if drift > JITTER_S {
                    // A hole: zeros, through the resampler like the rest.
                    let zeros = vec![0.0; (drift * rate).round() as usize];
                    self.feed(&zeros);
                    next + zeros.len() as f64 / rate
                } else if drift < -JITTER_S {
                    // An overlap: what was already heard is dropped.
                    let skip = ((-drift) * rate).round() as usize;
                    if skip >= input.len() {
                        return;
                    }
                    input = &input[skip..];
                    next
                } else {
                    next
                }
            }
            _ => {
                self.anchor(at);
                at
            }
        };
        self.feed(input);
        cursor += input.len() as f64 / rate;
        self.next_at = Some(cursor);
    }

    /// Start (again) at `at`: a fresh resampler, the timeline padded or cut
    /// to that point.
    fn anchor(&mut self, at: f64) {
        self.resampler = (self.rate != SAMPLE_RATE).then(|| To16k::new(self.rate));
        let target = to_samples(at);
        if target > self.produced {
            let zeros = vec![0; (target - self.produced) as usize];
            self.append(&zeros);
        } else if target < self.produced {
            let keep = target.saturating_sub(self.emitted) as usize;
            self.out.truncate(keep);
            self.produced = target;
        }
    }

    fn feed(&mut self, input: &[f32]) {
        let mut out = Vec::new();
        match &mut self.resampler {
            Some(resampler) => resampler.push(input, &mut out),
            None => out.extend_from_slice(input),
        }
        let pcm: Vec<i16> = out.iter().map(|s| to_i16(*s)).collect();
        self.append(&pcm);
    }

    fn append(&mut self, samples: &[i16]) {
        let skip = self.emitted.saturating_sub(self.produced).min(samples.len() as u64) as usize;
        self.out.extend(&samples[skip..]);
        self.produced += samples.len() as u64;
    }

    /// `len` samples from the front, zero-filled if this side is behind.
    fn take(&mut self, len: usize) -> Vec<i16> {
        let n = len.min(self.out.len());
        let mut samples: Vec<i16> = self.out.drain(..n).collect();
        samples.resize(len, 0);
        self.emitted += len as u64;
        samples
    }

    /// Flush the resampler; where this side really ends.
    fn finish(&mut self) -> u64 {
        let Some(next) = self.next_at else { return 0 };
        let end = to_samples(next);
        if let Some(resampler) = &mut self.resampler {
            let mut out = Vec::new();
            resampler.flush(&mut out);
            let pcm: Vec<i16> = out.iter().map(|s| to_i16(*s)).collect();
            self.append(&pcm);
        }
        if self.produced > end {
            let keep = end.saturating_sub(self.emitted) as usize;
            self.out.truncate(keep);
            self.produced = end;
        }
        end
    }
}

fn downmix(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }
    samples.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
}

fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

/// A fixed-ratio resampler to 16 kHz, mono, fed any number of samples at a
/// time, its start-up delay trimmed.
struct To16k {
    fft: Fft<f32>,
    pending: Vec<f32>,
    /// Output samples still to trim (the resampler's delay).
    trim: usize,
    buf: Vec<f32>,
}

impl To16k {
    fn new(rate: u32) -> Self {
        // 10 ms chunks: low delay, fine for speech.
        let chunk = (rate as usize / 100).max(1);
        let fft = Fft::<f32>::new(rate as usize, SAMPLE_RATE as usize, chunk, 1, FixedSync::Input)
            .expect("a resampler from a positive rate to 16 kHz");
        let trim = fft.output_delay();
        let buf = vec![0.0; fft.output_frames_max()];
        Self { fft, pending: Vec::new(), trim, buf }
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.pending.extend_from_slice(input);
        let mut used = 0;
        loop {
            let need = self.fft.input_frames_next();
            if self.pending.len() - used < need {
                break;
            }
            let chunk = &self.pending[used..used + need];
            let input = InterleavedSlice::new(chunk, 1, need).expect("a mono chunk");
            let frames = self.buf.len();
            let mut output = InterleavedSlice::new_mut(&mut self.buf, 1, frames).expect("a mono buffer");
            let (_, produced) = self
                .fft
                .process_into_buffer(&input, &mut output, None)
                .expect("a full chunk resamples");
            used += need;
            let trim = self.trim.min(produced);
            self.trim -= trim;
            out.extend_from_slice(&self.buf[trim..produced]);
        }
        self.pending.drain(..used);
    }

    /// Push out what is held back (the partial chunk and the delay).
    fn flush(&mut self, out: &mut Vec<f32>) {
        let need = self.fft.input_frames_next();
        let zeros = vec![0.0; need - self.pending.len() % need + need * 2];
        self.push(&zeros, out);
    }
}
