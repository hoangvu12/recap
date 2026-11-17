//! How the level rule (ADR 0021) judges a meeting's Me lines: for each line
//! of `live.ndjson` (or `refined.ndjson`) from the mic, how much of it was
//! the user (ms) and whether it counts as echo. Prints times and verdicts,
//! not the words: meetings are private.
//!
//!     cargo run -p recap-engine --example echo_check -- <meeting folder> [live|refined]

fn main() {
    use recap_engine::{Recording, mic_levels::MicLevels};

    let mut args = std::env::args().skip(1);
    let Some(folder) = args.next().map(std::path::PathBuf::from) else {
        eprintln!("usage: echo_check <meeting folder> [live|refined]");
        std::process::exit(2);
    };
    let which = args.next().unwrap_or_else(|| "live".into());
    let rec = Recording::read(&folder.join("audio.ogg")).expect("the recording decodes");
    let levels = MicLevels::from_recording(&rec.me, &rec.them);
    println!("threshold {:.1} dB under the meeting audio", -levels.threshold_db());
    let lines = std::fs::read_to_string(folder.join(format!("{which}.ndjson"))).expect("the transcript reads");
    let (mut kept, mut dropped) = (0, 0);
    for line in lines.lines() {
        let Ok(segment) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if segment["source"] != "me" {
            continue;
        }
        let (start, end) = (segment["start_ms"].as_u64().unwrap_or(0), segment["end_ms"].as_u64().unwrap_or(0));
        let words = segment["text"].as_str().unwrap_or("").split_whitespace().count();
        let (mine, known) = levels.mine_ms(start, end);
        let echo = levels.is_echo(start, end);
        if echo {
            dropped += 1
        } else {
            kept += 1
        }
        println!(
            "{:>8.1}s-{:>7.1}s  {words:>3} words  mine {mine:>5}/{known:>5} ms  {}",
            start as f64 / 1000.,
            end as f64 / 1000.,
            if echo { "ECHO" } else { "kept" }
        );
    }
    println!("{kept} kept, {dropped} echo");
}
