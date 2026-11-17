//! Echo by level (GLOSSARY: Echo dedup; ADR 0021). On laptop speakers the
//! mic hears the meeting audio; Windows' echo cancelling takes most of it out
//! but leaves a faint, reshaped trace that a speech engine still writes down,
//! in words that differ from what Them's side heard, so the text rule
//! ([`crate::echo`]) can't match it.
//!
//! So the levels decide too. Per 100 ms frame: the mic's level, and the loudest
//! the meeting audio was over the last [`LOOK_BACK`] frames (the echo comes
//! back ~200–250 ms late, with a tail). While Them speaks, the mic sits a
//! steady amount below them: the meeting's **echo gain**, its median. A frame
//! is **mine** when the mic is clearly above that (by [`MARGIN_DB`]) and above
//! its own noise floor. A Me segment with less than [`MINE_MS`] of mine frames
//! is an echo, whatever its words.
//!
//! Measured on a real call (laptop speakers, Teams-style echo cancelling):
//! Them -30..-40 dBFS, the mic during it ~-60 (gain about -26 dB, bursts to
//! -18), the user's own voice -32..-48, the mic's floor about -64.

use crate::{AudioFrame, Source};

/// A frame's length, ms (the timeline's frames).
pub const FRAME_MS: u64 = 100;
/// Frames back the meeting audio's loudest is taken over: the echo's lag
/// and tail.
pub const LOOK_BACK: usize = 5;
/// How far above the echo gain the mic must be for a frame to be mine, dB.
pub const MARGIN_DB: f32 = 12.0;
/// A frame is mine only this far above the mic's noise floor, dB.
pub const ABOVE_FLOOR_DB: f32 = 8.0;
/// A Me segment holding less of mine than this is an echo, ms.
pub const MINE_MS: u64 = 300;
/// ...or less than this share of it: a long echo row picks up a few loud
/// frames (a cough, a word said over Them), and someone speaking fills far
/// more of their own line (60% and more, measured).
pub const MINE_SHARE: f64 = 0.2;
/// Them counts as speaking above this, dBFS.
const THEM_SPEAKING_DB: f32 = -50.0;
/// The gain the threshold sits at, clamped: never so strict that speaking
/// over Them is lost, never so lax that echo bursts pass.
const THRESHOLD_DB: (f32, f32) = (-24.0, -6.0);
/// Before enough of the meeting is heard: lenient (most is mine).
const GAIN_UNTIL_KNOWN_DB: f32 = -18.0;
/// Samples the gain and the floor need before they are trusted.
const ENOUGH: u32 = 30;
const SILENT_DB: f32 = -100.0;

/// A dB histogram in half-dB bins, -100..+20: quantiles in O(bins).
#[derive(Clone, Debug)]
struct Histogram {
    bins: Vec<u32>,
    n: u32,
}

impl Default for Histogram {
    fn default() -> Self {
        Self { bins: vec![0; 240], n: 0 }
    }
}

impl Histogram {
    fn add(&mut self, db: f32) {
        let i = ((db - SILENT_DB) * 2.0).clamp(0.0, 239.0) as usize;
        self.bins[i] += 1;
        self.n += 1;
    }

    fn quantile(&self, q: f32) -> Option<f32> {
        if self.n < ENOUGH {
            return None;
        }
        let want = (self.n as f32 * q).ceil().max(1.0) as u32;
        let mut seen = 0;
        for (i, count) in self.bins.iter().enumerate() {
            seen += count;
            if seen >= want {
                return Some(SILENT_DB + (i as f32 + 0.5) / 2.0);
            }
        }
        None
    }
}

/// A frame's level, dBFS.
pub fn level_db(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return SILENT_DB;
    }
    let power = samples.iter().map(|&s| (f64::from(s) / 32768.0).powi(2)).sum::<f64>() / samples.len() as f64;
    (10.0 * power.max(1e-10).log10()) as f32
}

/// Both sides' levels over a meeting, frame by frame.
#[derive(Clone, Debug, Default)]
pub struct MicLevels {
    me: Vec<Option<f32>>,
    them: Vec<Option<f32>>,
    /// The mic against the meeting audio's recent loudest, while Them speaks.
    gain: Histogram,
    /// The mic's levels: its noise floor is a low quantile.
    floor: Histogram,
}

impl MicLevels {
    /// From a whole recording (each side 16 kHz mono, meeting time).
    pub fn from_recording(me: &[i16], them: &[i16]) -> Self {
        let mut levels = Self::default();
        let frame = (crate::SAMPLE_RATE as u64 * FRAME_MS / 1000) as usize;
        for (source, samples) in [(Source::Me, me), (Source::Them, them)] {
            for (i, chunk) in samples.chunks(frame).enumerate() {
                levels.set(source, i, level_db(chunk));
            }
        }
        levels
    }

    /// A frame heard live (paused audio isn't: it isn't transcribed either).
    pub fn push(&mut self, frame: &AudioFrame) {
        self.set(frame.source, (frame.start_ms / FRAME_MS) as usize, level_db(&frame.samples));
    }

    fn set(&mut self, source: Source, i: usize, db: f32) {
        let side = match source {
            Source::Me => &mut self.me,
            Source::Them => &mut self.them,
        };
        if side.len() <= i {
            side.resize(i + 1, None);
        }
        side[i] = Some(db);
        // Learnt once both sides of frame i are in.
        if let (Some(me), Some(_)) = (self.me.get(i).copied().flatten(), self.them.get(i).copied().flatten()) {
            self.floor.add(me);
            let peak = self.them_peak(i);
            if peak > THEM_SPEAKING_DB {
                self.gain.add(me - peak);
            }
        }
    }

    /// The meeting audio's loudest over frames i-LOOK_BACK..=i.
    fn them_peak(&self, i: usize) -> f32 {
        (i.saturating_sub(LOOK_BACK)..=i).filter_map(|j| self.them.get(j).copied().flatten()).fold(SILENT_DB, f32::max)
    }

    /// The mic's level below the meeting audio's (dB) a frame must beat.
    pub fn threshold_db(&self) -> f32 {
        let gain = self.gain.quantile(0.5).unwrap_or(GAIN_UNTIL_KNOWN_DB);
        (gain + MARGIN_DB).clamp(THRESHOLD_DB.0, THRESHOLD_DB.1)
    }

    /// Whether frame `i` is the user speaking (None: the mic's level there is
    /// unknown).
    fn mine(&self, i: usize, threshold: f32, floor: Option<f32>) -> Option<bool> {
        let me = self.me.get(i).copied().flatten()?;
        if floor.is_some_and(|floor| me < floor + ABOVE_FLOOR_DB) {
            return Some(false);
        }
        Some(me - self.them_peak(i) > threshold)
    }

    /// How much of `start_ms..end_ms` (and a frame either side: segment times
    /// are the words', the voice runs a little wider) is mine, and how much
    /// of it is known, ms.
    pub fn mine_ms(&self, start_ms: u64, end_ms: u64) -> (u64, u64) {
        let (threshold, floor) = (self.threshold_db(), self.floor.quantile(0.1));
        let (mut mine, mut known) = (0, 0);
        for i in Self::frames(start_ms, end_ms) {
            match self.mine(i, threshold, floor) {
                Some(true) => {
                    mine += FRAME_MS;
                    known += FRAME_MS;
                }
                Some(false) => known += FRAME_MS,
                None => {}
            }
        }
        (mine, known)
    }

    fn frames(start_ms: u64, end_ms: u64) -> std::ops::RangeInclusive<usize> {
        (start_ms / FRAME_MS).saturating_sub(1) as usize..=(end_ms / FRAME_MS + 1) as usize
    }

    /// Whether the mic, over `start_ms..end_ms`, only heard the meeting
    /// audio: Them spoke then, and the mic was hardly ever clearly above it
    /// (less than [`MINE_MS`], or than [`MINE_SHARE`] of the line). Levels
    /// unknown there, or Them quiet (nothing to echo), say no.
    pub fn is_echo(&self, start_ms: u64, end_ms: u64) -> bool {
        // Echo comes back late: Them speaking after the line can't be in it
        // (the look-back already covers the lag before it).
        let last = (end_ms.saturating_sub(1) / FRAME_MS) as usize;
        let them_spoke =
            Self::frames(start_ms, end_ms).take_while(|&i| i <= last).any(|i| self.them_peak(i) > THEM_SPEAKING_DB);
        let (mine, known) = self.mine_ms(start_ms, end_ms);
        them_spoke && known >= MINE_MS && (mine < MINE_MS || (mine as f64) < MINE_SHARE * known as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of a tone at `db` dBFS (0: silence).
    fn tone(db: f32) -> Vec<i16> {
        let n = (crate::SAMPLE_RATE as u64 * FRAME_MS / 1000) as usize;
        if db <= SILENT_DB {
            return vec![0; n];
        }
        let amp = 10f32.powf(db / 20.0) * std::f32::consts::SQRT_2 * 32768.0;
        (0..n).map(|k| (amp * (k as f32 * 0.3).sin()) as i16).collect()
    }

    /// A meeting from per-frame levels (dBFS) of each side.
    fn meeting(frames: &[(f32, f32)]) -> MicLevels {
        let mut levels = MicLevels::default();
        for (i, &(me, them)) in frames.iter().enumerate() {
            let start_ms = i as u64 * FRAME_MS;
            levels.push(&AudioFrame { source: Source::Me, start_ms, samples: tone(me) });
            levels.push(&AudioFrame { source: Source::Them, start_ms, samples: tone(them) });
        }
        levels
    }

    #[test]
    fn a_frame_level_is_its_rms_in_dbfs() {
        assert!((level_db(&tone(-30.0)) + 30.0).abs() < 0.5);
        assert_eq!(level_db(&[]), SILENT_DB);
    }

    /// 60 s of a call on laptop speakers: Them talks at -32 and the mic
    /// hears it back at -58 (gain -26), the mic's floor -64; then frames
    /// `mine` where the user says something at `me_db` over Them.
    fn call(me_db: f32, them_db: f32) -> MicLevels {
        let mut frames = Vec::new();
        for i in 0..600 {
            // Them talks 3 s, then is quiet 1 s.
            let talking = i % 40 < 30;
            frames.push(if talking { (-58.0, -32.0) } else { (-64.0, -75.0) });
        }
        // 1 s of the user: frames 600..610.
        for _ in 0..10 {
            frames.push((me_db, them_db));
        }
        // An echo burst at -46 while Them is at -30 (gain -16): frames 610..615.
        for _ in 0..5 {
            frames.push((-46.0, -30.0));
        }
        meeting(&frames)
    }

    #[test]
    fn the_mic_hearing_them_back_is_echo() {
        let levels = call(-58.0, -32.0);
        // The threshold follows the meeting's echo gain (-26 + 12).
        assert!((levels.threshold_db() + 14.0).abs() < 1.0, "{}", levels.threshold_db());
        assert!(levels.is_echo(10_000, 12_000), "plain echo");
        assert!(levels.is_echo(61_000, 61_500), "an echo burst louder than usual");
    }

    // Seen in a real call: a 34 s Me row of echo held 0.6 s of loud frames.
    #[test]
    fn a_long_echo_row_with_a_few_loud_frames_is_still_echo() {
        let mut frames = Vec::new();
        for i in 0..400 {
            frames.push(if i % 40 < 30 { (-58.0, -32.0) } else { (-64.0, -75.0) });
        }
        // Over 30 s of Them, the mic is clearly up for 600 ms (a cough).
        for frame in &mut frames[100..106] {
            *frame = (-36.0, -32.0);
        }
        let levels = meeting(&frames);
        assert_eq!(levels.mine_ms(5_000, 35_000).0, 600);
        assert!(levels.is_echo(5_000, 35_000));
        // The same 600 ms as a short line of its own is the user.
        assert!(!levels.is_echo(9_900, 10_700));
    }

    #[test]
    fn me_speaking_is_not_echo() {
        // Alone (Them quiet), over Them, and quietly over Them.
        assert!(!call(-40.0, -75.0).is_echo(60_000, 61_000));
        assert!(!call(-34.0, -32.0).is_echo(60_000, 61_000));
        assert!(!call(-44.0, -34.0).is_echo(60_000, 61_000));
    }

    #[test]
    fn noise_under_the_floor_is_not_me_but_not_echo_either() {
        let levels = call(-62.0, -75.0);
        assert_eq!(levels.mine_ms(60_000, 61_000).0, 0, "not mine");
        // Them quiet: there was nothing to echo; the words decide. Them
        // starting again right after the line (frame 610) doesn't count.
        assert!(!levels.is_echo(60_000, 61_000));
        // A silent meeting (scripted tests): nothing is judged by level.
        let silent = meeting(&vec![(-100.0, -100.0); 300]);
        assert!(!silent.is_echo(1_000, 5_000));
    }

    #[test]
    fn the_two_sides_alike_is_no_reason_to_drop() {
        // The test WAV: both channels the same audio. Levels can't tell;
        // the text rule does (crate::echo).
        let frames: Vec<(f32, f32)> =
            (0..300).map(|i| if i % 40 < 30 { (-30.0, -30.0) } else { (-90.0, -90.0) }).collect();
        let levels = meeting(&frames);
        assert!(!levels.is_echo(1_000, 2_000));
    }

    #[test]
    fn unknown_levels_drop_nothing() {
        let levels = MicLevels::default();
        assert!(!levels.is_echo(1_000, 5_000));
        assert_eq!(levels.mine_ms(1_000, 5_000), (0, 0));
    }

    #[test]
    fn a_recording_gives_the_same_levels_as_its_frames() {
        let frames: Vec<(f32, f32)> =
            (0..200).map(|i| if i % 40 < 30 { (-58.0, -32.0) } else { (-64.0, -75.0) }).collect();
        let live = meeting(&frames);
        let (me, them): (Vec<i16>, Vec<i16>) =
            frames.iter().flat_map(|&(m, t)| tone(m).into_iter().zip(tone(t))).unzip();
        let recorded = MicLevels::from_recording(&me, &them);
        assert_eq!(recorded.threshold_db(), live.threshold_db());
        assert_eq!(recorded.mine_ms(0, 20_000), live.mine_ms(0, 20_000));
    }
}
