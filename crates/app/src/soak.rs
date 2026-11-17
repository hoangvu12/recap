//! The soak run: `recap --soak <minutes>`. The whole app runs one endless
//! meeting: `testdata/meeting_tts.wav` again and again in real time, the
//! scripted captions provider looping with it (no real API), recorded to
//! disk, Notes and the overlay open. Every minute it samples the process's
//! private bytes, handles and GDI/USER objects into `soak.csv`; at the end it
//! stops the meeting, judges the samples ([`verdict`]), writes
//! `soak_result.txt` and quits (exit code 1 when something grew).
//!
//! Its home is `RECAP_DATA`, else `%TEMP%\recap-soak` (never the real
//! meetings), and it is its own instance: the user's Recap may keep running.

use std::{
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::live::{Audio, StartOptions};

/// What `--soak` asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Soak {
    pub minutes: u32,
    /// The meeting played in a loop (`--replay <wav>`, else the test meeting).
    pub wav: PathBuf,
}

impl Soak {
    /// `--soak <minutes>` (and `--replay <wav>` for another file), or `None`.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Option<Soak> {
        let args: Vec<String> = args.into_iter().collect();
        let after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1));
        let minutes = after("--soak")?.parse().ok().filter(|m| *m > 0)?;
        let wav = after("--replay").map_or_else(test_meeting, PathBuf::from);
        Some(Soak { minutes, wav })
    }

    /// How its meeting starts: the WAV looped, scripted captions.
    pub fn start_options(&self) -> StartOptions {
        StartOptions { audio: Audio::Looped(self.wav.clone()), scripted: true }
    }

    /// Where it keeps its settings and meetings.
    pub fn home() -> PathBuf {
        std::env::var_os("RECAP_DATA").map_or_else(|| std::env::temp_dir().join("recap-soak"), PathBuf::from)
    }

    /// Sample now and every minute for `minutes` into `<home>/soak.csv`, on
    /// a thread of its own; then judge, write `soak_result.txt` and hand the
    /// verdict to `done`.
    pub fn watch(&self, home: &Path, done: impl FnOnce(Verdict) + Send + 'static) -> std::io::Result<()> {
        std::fs::create_dir_all(home)?;
        let (csv_path, result_path) = (home.join("soak.csv"), home.join("soak_result.txt"));
        let mut csv = std::fs::File::create(&csv_path)?;
        writeln!(csv, "{}", Sample::HEADER)?;
        let minutes = self.minutes;
        std::thread::Builder::new().name("recap-soak".into()).spawn(move || {
            let started = Instant::now();
            let mut samples = Vec::new();
            for minute in 0..=minutes {
                let due = started + Duration::from_secs(60 * minute as u64);
                if let Some(wait) = due.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
                let sample = Sample::now(minute);
                let _ = writeln!(csv, "{}", sample.csv()).and_then(|_| csv.flush());
                eprintln!("recap: soak {}", sample.csv());
                samples.push(sample);
                // So far, in case the run ends early (quit, killed).
                if minute < minutes {
                    let so_far = verdict(&samples, &Limits::default()).report();
                    let _ = std::fs::write(&result_path, format!("RUNNING minute {minute} of {minutes}: {so_far}"));
                }
            }
            let verdict = verdict(&samples, &Limits::default());
            let _ = std::fs::write(&result_path, verdict.report());
            eprintln!("recap: soak {}", verdict.report());
            done(verdict);
        })?;
        Ok(())
    }
}

fn test_meeting() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/meeting_tts.wav")
}

/// The process, one minute into the run.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub minute: u32,
    pub private_bytes: u64,
    pub handles: u32,
    pub gdi_objects: u32,
    pub user_objects: u32,
}

impl Sample {
    pub const HEADER: &str = "minute,private_mib,handles,gdi,user";

    pub fn csv(&self) -> String {
        format!(
            "{},{:.1},{},{},{}",
            self.minute,
            self.private_bytes as f64 / MIB,
            self.handles,
            self.gdi_objects,
            self.user_objects
        )
    }

    /// This process, now.
    #[cfg(windows)]
    pub fn now(minute: u32) -> Sample {
        use windows::Win32::System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
            Threading::{GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources, GetProcessHandleCount},
        };
        // SAFETY: the pseudo handle of this process needs no closing; the
        // counters are written into a struct of the size given.
        unsafe {
            let process = GetCurrentProcess();
            let mut memory = PROCESS_MEMORY_COUNTERS_EX {
                cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                ..Default::default()
            };
            let _ = GetProcessMemoryInfo(
                process,
                (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(),
                memory.cb,
            );
            let mut handles = 0;
            let _ = GetProcessHandleCount(process, &mut handles);
            Sample {
                minute,
                private_bytes: memory.PrivateUsage as u64,
                handles,
                gdi_objects: GetGuiResources(process, GR_GDIOBJECTS),
                user_objects: GetGuiResources(process, GR_USEROBJECTS),
            }
        }
    }

    #[cfg(not(windows))]
    pub fn now(minute: u32) -> Sample {
        Sample { minute, ..Sample::default() }
    }
}

const MIB: f64 = 1024. * 1024.;

/// How much growth an hour is a leak, once warm.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Samples before this minute are start-up, not judged.
    pub warmup_minutes: u32,
    pub private_mib_per_hour: f64,
    pub handles_per_hour: f64,
    /// GDI and USER objects each (zgx once leaked a DC per DPI read).
    pub gui_objects_per_hour: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { warmup_minutes: 5, private_mib_per_hour: 8., handles_per_hour: 60., gui_objects_per_hour: 20. }
    }
}

/// Whether the run stayed flat, and each measure's growth.
#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    pub ok: bool,
    pub lines: Vec<String>,
}

impl Verdict {
    pub fn report(&self) -> String {
        let mut out = String::from(if self.ok { "PASS\n" } else { "FAIL\n" });
        for line in &self.lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

/// Judge `samples`: a measure grew if its trend after the warm-up is over its
/// limit an hour, both over the whole warm stretch and over its second half
/// (sustained: start-up that runs a little past the warm-up, then levels
/// off, passes). Too few warm samples to judge passes, saying so.
pub fn verdict(samples: &[Sample], limits: &Limits) -> Verdict {
    let warm: Vec<&Sample> = samples.iter().filter(|s| s.minute >= limits.warmup_minutes).collect();
    if warm.len() < 4 {
        return Verdict { ok: true, lines: vec![format!("too short to judge: {} warm samples", warm.len())] };
    }
    let late = &warm[warm.len() / 2..];
    let measures: [(&str, fn(&Sample) -> f64, f64, &str); 4] = [
        ("private bytes", |s| s.private_bytes as f64 / MIB, limits.private_mib_per_hour, "MiB"),
        ("handles", |s| s.handles as f64, limits.handles_per_hour, ""),
        ("GDI objects", |s| s.gdi_objects as f64, limits.gui_objects_per_hour, ""),
        ("USER objects", |s| s.user_objects as f64, limits.gui_objects_per_hour, ""),
    ];
    let mut ok = true;
    let mut lines = Vec::new();
    for (name, value, limit, unit) in measures {
        let trend = |points: &[&Sample]| per_hour(&points.iter().map(|s| (s.minute as f64, value(s))).collect::<Vec<_>>());
        let (all, second_half) = (trend(&warm), trend(late));
        let grew = all > limit && second_half > limit;
        ok &= !grew;
        let (first, last) = (value(warm[0]), value(warm[warm.len() - 1]));
        lines.push(format!(
            "{name}: {first:.1} -> {last:.1}{unit} (minutes {}..{}), trend {all:+.1}{unit}/h, second half {second_half:+.1}{unit}/h, limit {limit}{unit}/h{}",
            warm[0].minute,
            warm[warm.len() - 1].minute,
            if grew { "  GREW" } else { "" }
        ));
    }
    Verdict { ok, lines }
}

/// The least-squares slope of `(minute, value)` points, per hour.
fn per_hour(points: &[(f64, f64)]) -> f64 {
    let n = points.len() as f64;
    if n < 2. {
        return 0.;
    }
    let (mx, my) = (points.iter().map(|p| p.0).sum::<f64>() / n, points.iter().map(|p| p.1).sum::<f64>() / n);
    let (num, den) = points
        .iter()
        .fold((0., 0.), |(num, den), (x, y)| (num + (x - mx) * (y - my), den + (x - mx) * (x - mx)));
    if den == 0. { 0. } else { num / den * 60. }
}
