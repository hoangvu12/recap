//! Decode a meeting's `audio.ogg` to a stereo WAV (left: the mic, Me;
//! right: the meeting audio, Them), 16 kHz, to listen to it or replay it
//! (`recap --replay out.wav`). Keep the output out of the repo: it is a
//! private meeting.
//!
//!     cargo run -p recap-engine --example decode -- audio.ogg out.wav

fn main() {
    use recap_engine::{Recording, SAMPLE_RATE};

    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: decode <audio.ogg> <out.wav>");
        std::process::exit(2);
    };
    let rec = Recording::read(std::path::Path::new(&input)).expect("it decodes");
    let spec =
        hound::WavSpec { channels: 2, sample_rate: SAMPLE_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut wav = hound::WavWriter::create(&output, spec).expect("the WAV opens");
    let n = rec.me.len().max(rec.them.len());
    for i in 0..n {
        wav.write_sample(rec.me.get(i).copied().unwrap_or(0)).expect("written");
        wav.write_sample(rec.them.get(i).copied().unwrap_or(0)).expect("written");
    }
    wav.finalize().expect("closed");
    println!("{:.1} s to {output}", n as f64 / f64::from(SAMPLE_RATE));
}
