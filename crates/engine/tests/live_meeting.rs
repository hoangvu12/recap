//! Seam 1: a whole meeting through the engine, from an AudioSource and a
//! LiveStt provider to the events it emits and the MeetingResult it ends with.

use std::sync::Arc;
use std::time::Duration;

use recap_engine::{
    EngineEvent, FrameFeed, Meeting, MeetingConfig, MeetingResult, MeetingState, Script, ScriptedLiveStt, Segment,
    SilenceSource, Source,
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

fn start(source: impl recap_engine::AudioSource + 'static) -> Meeting {
    Meeting::start(MeetingConfig::default(), Box::new(source), Arc::new(ScriptedLiveStt::new(script())))
}

#[tokio::test]
async fn a_replayed_script_becomes_partials_then_finals_with_me_and_them() {
    let mut meeting = start(SilenceSource::new(Duration::from_secs(150)));
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
        let partials: Vec<&&Segment> = all.iter().filter(|s| s.id == fin.id && !s.is_final).collect();
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
async fn both_sides_are_always_transcribed_live() {
    // "Caption my own speech" only decides the overlay; the engine always
    // transcribes Me for the Notes transcript (story 21).
    let mut meeting = start(SilenceSource::new(Duration::from_secs(150)));
    let (_, result) = drain(&mut meeting).await;
    let script = script();
    let mine: Vec<&str> = script.lines.iter().filter(|l| l.voice == "Mark").map(|l| l.text.as_str()).collect();
    assert!(!mine.is_empty());
    let rows: Vec<&str> =
        result.rows.iter().filter(|r| r.source == Source::Me).map(|r| r.final_text.as_str()).collect();
    assert_eq!(rows, mine, "a row per line of mine");
    // Its finals (a line may be several) are what I said.
    let finals: Vec<&str> =
        result.segments.iter().filter(|s| s.source == Source::Me).map(|s| s.text.as_str()).collect();
    assert!(finals.len() >= mine.len(), "{finals:?}");
    assert!(finals.iter().all(|f| mine.iter().any(|line| line.contains(f))), "{finals:?}");
}

#[tokio::test]
async fn elapsed_ticks_once_a_second_of_audio() {
    let mut meeting = start(SilenceSource::new(Duration::from_millis(3_500)));
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
    let mut meeting = start(source);

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
    let mut meeting = start(source);
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

/// A provider that hears `me` on the mic and `them` on the meeting audio,
/// each as partials then a final, Me `me_lag_ms` after Them.
struct TwoLines {
    me: &'static str,
    them: &'static str,
    me_lag_ms: u64,
}

impl recap_engine::LiveStt for TwoLines {
    fn name(&self) -> &str {
        "Two lines"
    }
    fn capabilities(&self) -> recap_engine::Capabilities {
        recap_engine::Capabilities { live: true, ..Default::default() }
    }
    fn run(
        &self,
        source: Source,
        _options: recap_engine::LiveOptions,
        mut audio: tokio::sync::mpsc::UnboundedReceiver<recap_engine::AudioFrame>,
        events: tokio::sync::mpsc::UnboundedSender<recap_engine::LiveEvent>,
    ) -> recap_engine::BoxFuture<'static, Result<(), recap_engine::ProviderError>> {
        let (text, lag) = match source {
            Source::Me => (self.me, self.me_lag_ms),
            Source::Them => (self.them, 0),
        };
        let words: Vec<&str> = text.split(' ').collect();
        let start = 1_000 + lag;
        let mut schedule: Vec<recap_engine::LiveEvent> = (1..=words.len())
            .map(|i| recap_engine::LiveEvent {
                text: words[..i].join(" "),
                is_final: false,
                start_ms: start,
                end_ms: start + i as u64 * 300,
            })
            .collect();
        let end = start + words.len() as u64 * 300 + 200;
        schedule.push(recap_engine::LiveEvent { text: text.into(), is_final: true, start_ms: start, end_ms: end });
        Box::pin(async move {
            let mut heard = 0;
            let mut next = 0;
            while let Some(frame) = audio.recv().await {
                heard += frame.duration_ms();
                while let Some(event) = schedule.get(next).filter(|e| e.end_ms <= heard) {
                    let _ = events.send(event.clone());
                    next += 1;
                }
            }
            Ok(())
        })
    }
}

fn two_lines(me: &'static str, them: &'static str) -> Meeting {
    Meeting::start(
        MeetingConfig::default(),
        Box::new(SilenceSource::new(Duration::from_secs(12))),
        Arc::new(TwoLines { me, them, me_lag_ms: 150 }),
    )
}

const LAUNCH: &str = "Then I'd suggest we launch on December first, with caching in place.";

#[tokio::test]
async fn the_mic_hearing_them_through_the_speakers_is_dropped() {
    let mut meeting = two_lines("then I'd suggest we launch on December first with caching in place", LAUNCH);
    let (events, result) = drain(&mut meeting).await;

    // Nothing of Me is kept: not in the result, never committed.
    assert!(result.segments.iter().all(|s| s.source == Source::Them), "{:?}", result.segments);
    assert_eq!(result.segments.len(), 1);
    assert_eq!(committed(&events), vec![(Source::Them, LAUNCH.to_string())]);
    // What of Me was shown is taken back.
    let shown: std::collections::BTreeSet<_> =
        segments(&events).iter().filter(|s| s.source == Source::Me).map(|s| s.id).collect();
    let dropped: std::collections::BTreeSet<_> = events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::Dropped(id) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(shown, dropped);
    // Its row, if one was made, holds no words.
    for row in result.rows.iter().filter(|r| r.source == Source::Me) {
        assert_eq!((row.final_text.as_str(), row.partial_text.as_str()), ("", ""), "{row:?}");
    }
    assert_eq!(result.rows.iter().filter(|r| r.source == Source::Them).count(), 1);
}

#[tokio::test]
async fn my_own_words_at_the_same_time_are_kept() {
    let mut meeting = two_lines("Sorry, can you repeat the date for the launch?", LAUNCH);
    let (events, result) = drain(&mut meeting).await;
    assert_eq!(result.segments.len(), 2);
    let mine = result.segments.iter().find(|s| s.source == Source::Me).expect("my line is kept");
    assert_eq!(mine.text, "Sorry, can you repeat the date for the launch?");
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::Dropped(_))));
    assert_eq!(committed(&events).len(), 2);
}

fn committed(events: &[EngineEvent]) -> Vec<(Source, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::Committed(s) => Some((s.source, s.text.clone())),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn pause_and_resume_leave_one_marker_in_the_transcript() {
    let (feed, source) = FrameFeed::new();
    let mut meeting = start(source);
    feed.push_silence(Duration::from_secs(2));
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::Elapsed(2_000))).await;

    let before = now_ms();
    meeting.pause();
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::State(MeetingState::Paused))).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    meeting.resume();
    // Resume marks the pause: where in the meeting, and when (wall clock).
    let mark = loop {
        let event = meeting.next_event().await.unwrap();
        if let EngineEvent::Pause(mark) = event {
            break mark;
        }
    };
    assert_eq!(mark.at_ms, 2_000);
    assert!(mark.paused_ms >= before && mark.resumed_ms >= mark.paused_ms + 300, "{mark:?}");
    assert!(mark.resumed_ms <= now_ms());

    feed.push_silence(Duration::from_secs(1));
    drop(feed);
    let (events, result) = drain(&mut meeting).await;
    // One meeting: one Recording → … → Stopped, the marker kept, its row in place.
    assert_eq!(result.duration_ms, 3_000);
    assert_eq!(result.pauses, vec![mark]);
    let rows: Vec<_> = result.rows.iter().filter(|r| r.pause.is_some()).collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pause, Some(mark));
    assert_eq!(rows[0].start_ms, 2_000);
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::Pause(_))), "one marker");
}

#[tokio::test]
async fn stop_while_paused_still_marks_the_pause() {
    let (feed, source) = FrameFeed::new();
    let mut meeting = start(source);
    feed.push_silence(Duration::from_secs(1));
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::Elapsed(1_000))).await;
    meeting.pause();
    wait_for(&mut meeting, |e| matches!(e, EngineEvent::State(MeetingState::Paused))).await;
    meeting.stop();
    let (events, result) = drain(&mut meeting).await;
    assert!(events.iter().any(|e| matches!(e, EngineEvent::Pause(m) if m.at_ms == 1_000)));
    assert_eq!(result.pauses.len(), 1);
    drop(feed);
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
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
