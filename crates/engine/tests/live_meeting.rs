//! Seam 1: a whole meeting through the engine, from an AudioSource and a
//! LiveStt provider to the events it emits and the MeetingResult it ends with.

use std::sync::Arc;
use std::time::Duration;

use recap_engine::{
    EngineEvent, FrameFeed, Meeting, MeetingConfig, MeetingResult, MeetingState, Script,
    ScriptedLiveStt, Segment, SilenceSource, Source,
};

const SCRIPT: &str = include_str!("../../../testdata/meeting_tts_script.txt");

fn script() -> Script {
    Script::parse(SCRIPT, "Mark").expect("the test script parses")
}

/// Every event until `Finished`, and the result it carries.
async fn drain(meeting: &mut Meeting) -> (Vec<EngineEvent>, MeetingResult) {
    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), meeting.next_event())
            .await
            .expect("the engine keeps talking")
            .expect("the stream ends with Finished, not silence");
        if let EngineEvent::Finished(result) = event {
            return (events, result);
        }
        events.push(event);
    }
}

fn segments(events: &[EngineEvent]) -> Vec<&Segment> {
    events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::Segment(s) => Some(s),
            _ => None,
        })
        .collect()
}

fn states(events: &[EngineEvent]) -> Vec<MeetingState> {
    events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::State(s) => Some(*s),
            _ => None,
        })
        .collect()
}

fn start(source: impl recap_engine::AudioSource + 'static, caption_me: bool) -> Meeting {
    Meeting::start(
        MeetingConfig { caption_me, ..MeetingConfig::default() },
        Box::new(source),
        Arc::new(ScriptedLiveStt::new(script())),
    )
}

#[tokio::test]
async fn a_replayed_script_becomes_partials_then_finals_with_me_and_them() {
    let mut meeting = start(SilenceSource::new(Duration::from_secs(150)), true);
    let (events, result) = drain(&mut meeting).await;

    assert_eq!(states(&events).first(), Some(&MeetingState::Recording));
    assert_eq!(states(&events).last(), Some(&MeetingState::Stopped));

    // Each script line becomes one row (turn) of the live transcript, said by
    // the right side: Mark's lines are Me (the mic), the others Them.
    let script = script();
    assert_eq!(result.rows.len(), script.lines.len());
    for (row, line) in result.rows.iter().zip(&script.lines) {
        assert_eq!(row.final_text, line.text, "row {} holds its line", row.index);
        assert_eq!(row.partial_text, "");
        let want = if line.voice == "Mark" { Source::Me } else { Source::Them };
        assert_eq!(row.source, want, "{}", line.text);
    }
    // Rows start in order, the second about 11 s in (the first line's words).
    let starts: Vec<u64> = result.rows.iter().map(|r| r.start_ms).collect();
    assert!(starts.windows(2).all(|w| w[0] < w[1]), "{starts:?}");
    assert!((9_000..14_000).contains(&starts[1]), "{starts:?}");

    // A segment is shown partial first, growing, then final with the same id;
    // its partials are prefixes of what it becomes.
    let all = segments(&events);
    for fin in all.iter().filter(|s| s.is_final) {
        let partials: Vec<&&Segment> =
            all.iter().filter(|s| s.id == fin.id && !s.is_final).collect();
        assert!(!partials.is_empty(), "{:?} had partials", fin.text);
        for p in &partials {
            assert!(fin.text.starts_with(&p.text), "{:?} grows into {:?}", p.text, fin.text);
            assert_eq!(p.source, fin.source);
        }
        let lens: Vec<usize> = partials.iter().map(|p| p.text.len()).collect();
        assert!(lens.windows(2).all(|w| w[0] < w[1]), "partials grow: {lens:?}");
    }
    assert_eq!(result.segments.len(), all.iter().filter(|s| s.is_final).count());
}

#[tokio::test]
async fn with_my_captions_off_only_them_is_transcribed_live() {
    let mut meeting = start(SilenceSource::new(Duration::from_secs(150)), false);
    let (events, result) = drain(&mut meeting).await;
    assert!(segments(&events).iter().all(|s| s.source == Source::Them));
    assert!(result.rows.iter().all(|r| r.source == Source::Them));
    assert_eq!(result.rows.len(), script().lines.iter().filter(|l| l.voice != "Mark").count());
}

#[tokio::test]
async fn elapsed_ticks_once_a_second_of_audio() {
    let mut meeting = start(SilenceSource::new(Duration::from_millis(3_500)), true);
    let (events, result) = drain(&mut meeting).await;
    let ticks: Vec<u64> = events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::Elapsed(ms) => Some(*ms),
            _ => None,
        })
        .collect();
    assert_eq!(ticks, vec![1_000, 2_000, 3_000]);
    assert_eq!(result.duration_ms, 3_500);
}

#[tokio::test]
async fn pause_holds_the_timer_and_captions_until_resume() {
    let (feed, source) = FrameFeed::new();
    let mut meeting = start(source, true);

    feed.push_silence(Duration::from_secs(2));
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::Elapsed(2_000))).await;

    meeting.pause();
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::State(MeetingState::Paused))).await;
    // Audio keeps coming while paused; neither captions nor the recording take it.
    feed.push_silence(Duration::from_secs(20));
    // A command applies when it arrives: let the engine read the paused audio
    // first (it takes it as fast as it can).
    tokio::time::sleep(Duration::from_millis(300)).await;
    meeting.resume();
    let mut during = Vec::new();
    loop {
        let event = meeting.next_event().await.unwrap();
        if matches!(event, EngineEvent::State(MeetingState::Recording)) {
            break;
        }
        during.push(event);
    }
    // Nothing moves while paused: no ticks, and no words from paused audio (a
    // segment still in flight from before the pause may land).
    assert!(
        during.iter().all(|e| match e {
            EngineEvent::Elapsed(_) => false,
            EngineEvent::Segment(s) => s.end_ms <= 2_000,
            _ => true,
        }),
        "nothing moves while paused: {during:?}"
    );

    feed.push_silence(Duration::from_secs(1));
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::Elapsed(3_000))).await;
    drop(feed);
    let (_, result) = drain(&mut meeting).await;
    assert_eq!(result.duration_ms, 3_000);
}

#[tokio::test]
async fn stop_finishes_with_what_was_said_so_far() {
    let (feed, source) = FrameFeed::new();
    let mut meeting = start(source, true);
    // The first line (Me) takes about 12 s; 15 s gets it final and Them started.
    feed.push_silence(Duration::from_secs(15));
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::Elapsed(15_000))).await;
    meeting.stop();
    let (events, result) = drain(&mut meeting).await;
    assert_eq!(states(&events).last(), Some(&MeetingState::Stopped));
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].final_text, script().lines[0].text);
    // What was still partial at Stop is kept as final text.
    assert!(!result.rows[1].final_text.is_empty());
    assert_eq!(result.rows[1].partial_text, "");
    assert!(meeting.next_event().await.is_none(), "nothing after Finished");
    drop(feed);
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("recap-meeting-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn recorded(source: impl recap_engine::AudioSource + 'static, path: &std::path::Path) -> Meeting {
    Meeting::start(
        MeetingConfig { recording: Some(path.to_path_buf()), ..MeetingConfig::default() },
        Box::new(source),
        Arc::new(ScriptedLiveStt::new(script())),
    )
}

#[tokio::test]
async fn the_test_meeting_replayed_from_its_wav_is_recorded_on_both_channels() {
    let wav = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/meeting_tts.wav");
    let path = scratch("replay.ogg");
    let mut meeting = recorded(recap_engine::WavSource::open(std::path::Path::new(wav)).unwrap(), &path);
    let (_, result) = drain(&mut meeting).await;
    assert!((118_000..=118_500).contains(&result.duration_ms), "{}", result.duration_ms);

    // The file is complete once Finished is out.
    let rec = recap_engine::Recording::read(&path).unwrap();
    let secs = rec.them.len() as f64 / recap_engine::SAMPLE_RATE as f64;
    assert!((secs * 1000.0 - result.duration_ms as f64).abs() < 200.0, "{secs} s on disk");
    let level = |s: &[i16]| s.iter().map(|v| v.unsigned_abs() as f64).sum::<f64>() / s.len() as f64;
    assert!(level(&rec.me) > 300.0 && level(&rec.them) > 300.0, "both channels carry the meeting");
}

#[tokio::test]
async fn paused_audio_is_left_out_of_the_recording() {
    let path = scratch("paused.ogg");
    let (feed, source) = FrameFeed::new();
    let mut meeting = recorded(source, &path);
    feed.push_silence(Duration::from_secs(2));
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::Elapsed(2_000))).await;
    meeting.pause();
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::State(MeetingState::Paused))).await;
    feed.push_silence(Duration::from_secs(5));
    tokio::time::sleep(Duration::from_millis(300)).await;
    meeting.resume();
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::State(MeetingState::Recording))).await;
    feed.push_silence(Duration::from_secs(1));
    drop(feed);
    let (_, result) = drain(&mut meeting).await;

    let rec = recap_engine::Recording::read(&path).unwrap();
    let ms = rec.me.len() as u64 * 1000 / recap_engine::SAMPLE_RATE as u64;
    // The file's time is meeting time: a line's start_ms finds it in the file.
    assert_eq!(result.duration_ms, 3_000);
    assert!(ms.abs_diff(3_000) < 100, "{ms} ms recorded");
}

async fn wait_for(meeting: &mut Meeting, want: impl Fn(&EngineEvent) -> bool) {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), meeting.next_event())
            .await
            .expect("the awaited event arrives")
            .expect("the meeting is still going");
        if want(&event) {
            return;
        }
    }
}
