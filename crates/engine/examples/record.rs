//! Record the mic and the meeting audio for a while, the way a meeting does,
//! then read the file back and print each channel's level per second.
//!
//!     cargo run -p recap-engine --example record -- out.ogg 60

#[cfg(windows)]
fn main() {
    use recap_engine::{AudioSource, LiveCapture, Recorder, Recording, SAMPLE_RATE, Source};

    let mut args = std::env::args().skip(1);
    let path = std::path::PathBuf::from(args.next().unwrap_or_else(|| "record.ogg".into()));
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);

    let mut capture = LiveCapture::start().expect("capture starts");
    let mut recorder = Recorder::create(&path).expect("the file opens");
    println!("recording {seconds} s to {}", path.display());
    while let Some(frame) = capture.next_frame() {
        recorder.write(&frame).expect("written");
        if frame.source == Source::Them && frame.start_ms + frame.duration_ms() >= seconds * 1000 {
            break;
        }
    }
    drop(capture);
    recorder.finish().expect("closed");

    let rec = Recording::read(&path).expect("it decodes");
    let second = SAMPLE_RATE as usize;
    let level = |s: &[i16]| s.iter().map(|v| v.unsigned_abs() as u64).sum::<u64>() / s.len().max(1) as u64;
    println!("{:.1} s back; level per second (mic | meeting audio):", rec.me.len() as f64 / second as f64);
    for (i, (me, them)) in rec.me.chunks(second).zip(rec.them.chunks(second)).enumerate() {
        println!("{i:>4}  {:>6} | {:>6}", level(me), level(them));
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("live capture is Windows only");
}
