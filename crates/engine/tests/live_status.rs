//! How live captions are doing, told while the meeting runs (the overlay's
//! Live / Reconnecting / error states): a provider's status, its failure
//! the moment it happens, switching provider mid-meeting, and my captions
//! turned on or off mid-meeting.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use recap_engine::{
    AudioFrame, BoxFuture, Capabilities, EngineEvent, FrameFeed, LiveEvent, LiveOptions, LiveStatus, LiveStt,
    Meeting, MeetingConfig, MeetingState, ProviderError, ProviderErrorKind, Source,
};
use tokio::sync::mpsc;

/// A provider that, per side, says `line` once it has heard `after_ms` of
/// audio; it may fail first, and may report statuses along the way.
#[derive(Clone)]
struct Line {
    name: &'static str,
    line: &'static str,
    /// What it says on my side (the same words would be an echo of Them).
    my_line: &'static str,
    after_ms: u64,
    /// Fail Them with this once `after_ms` is heard, instead of speaking (Me
    /// then stays quiet, and doesn't fail).
    fail: Option<ProviderErrorKind>,
    /// Report Reconnecting, then Connected, before speaking.
    blips: bool,
    /// The sides it was asked to caption.
    sides: Arc<Mutex<Vec<Source>>>,
}

impl Line {
    fn new(name: &'static str, line: &'static str) -> Self {
        Line { name, line, my_line: "Sounds good to me, thanks.", after_ms: 1_000, fail: None, blips: false, sides: Arc::default() }
    }
}

impl LiveStt for Line {
    fn name(&self) -> &str {
        self.name
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { live: true, ..Default::default() }
    }
    fn run(
        &self,
        source: Source,
        options: LiveOptions,
        audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let (status, _) = mpsc::unbounded_channel();
        self.run_reporting(source, options, audio, events, status)
    }
    fn run_reporting(
        &self,
        source: Source,
        _options: LiveOptions,
        mut audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
        status: mpsc::UnboundedSender<LiveStatus>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        self.sides.lock().unwrap().push(source);
        let me = self.clone();
        Box::pin(async move {
            let mut heard = 0;
            let mut said = false;
            while let Some(frame) = audio.recv().await {
                heard += frame.duration_ms();
                if heard >= me.after_ms && !said {
                    said = true;
                    if let Some(kind) = me.fail {
                        if source == Source::Me {
                            continue;
                        }
                        return Err(ProviderError::new(kind, "out of credit"));
                    }
                    if me.blips {
                        let _ = status.send(LiveStatus::Reconnecting);
                        let _ = status.send(LiveStatus::Connected);
                    }
                    let start = frame.start_ms;
                    let text = if source == Source::Me { me.my_line } else { me.line };
                    let _ = events.send(LiveEvent { text: text.into(), is_final: true, start_ms: start, end_ms: start + 100 });
                }
            }
            Ok(())
        })
    }
}

fn start(live: Line) -> (FrameFeed, Meeting) {
    let (feed, source) = FrameFeed::new();
    let meeting = Meeting::start(MeetingConfig::default(), Box::new(source), Arc::new(live));
    (feed, meeting)
}

/// Events until one matches `want` (that one included).
async fn until(meeting: &mut Meeting, want: impl Fn(&EngineEvent) -> bool) -> Vec<EngineEvent> {
    let mut seen = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), meeting.next_event())
            .await
            .expect("the awaited event arrives")
            .expect("the meeting is still going");
        let done = want(&event);
        seen.push(event);
        if done {
            return seen;
        }
    }
}

fn said(event: &EngineEvent, text: &str) -> bool {
    matches!(event, EngineEvent::Segment(s) if s.text == text && s.is_final)
}

#[tokio::test]
async fn each_side_says_which_provider_captions_it() {
    let (feed, mut meeting) = start(Line::new("ElevenLabs", "Hello."));
    let events = until(&mut meeting, |e| matches!(e, EngineEvent::Live { .. })).await;
    assert!(matches!(
        events.last(),
        Some(EngineEvent::Live { source: Source::Them, status: LiveStatus::Using(name) }) if name == "ElevenLabs"
    ));
    drop(feed);
}

#[tokio::test]
async fn a_provider_failing_is_told_at_once_while_recording_goes_on() {
    let live = Line { fail: Some(ProviderErrorKind::Quota), ..Line::new("ElevenLabs", "") };
    let (feed, mut meeting) = start(live);
    feed.push_silence(Duration::from_secs(2));
    let events = until(&mut meeting, |e| matches!(e, EngineEvent::Error { .. })).await;
    let Some(EngineEvent::Error { source, error }) = events.last() else { unreachable!() };
    assert_eq!((*source, error.kind), (Source::Them, ProviderErrorKind::Quota));
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::State(MeetingState::Stopped))), "told before Stop");

    // Recording goes on: the timer keeps counting.
    feed.push_silence(Duration::from_secs(2));
    until(&mut meeting, |e| matches!(e, EngineEvent::Elapsed(4_000))).await;
    drop(feed);
    let rest = until(&mut meeting, |e| matches!(e, EngineEvent::Finished(_))).await;
    assert!(!rest.iter().any(|e| matches!(e, EngineEvent::Error { .. })), "the error is told once");
}

#[tokio::test]
async fn switching_provider_after_a_failure_brings_captions_back() {
    let live = Line { fail: Some(ProviderErrorKind::Auth), ..Line::new("ElevenLabs", "") };
    let (feed, mut meeting) = start(live);
    feed.push_silence(Duration::from_secs(2));
    until(&mut meeting, |e| matches!(e, EngineEvent::Error { .. })).await;

    meeting.control().replace_live(Arc::new(Line::new("Deepgram", "We're back.")));
    let events = until(&mut meeting, |e| matches!(e, EngineEvent::Live { .. })).await;
    assert!(matches!(
        events.last(),
        Some(EngineEvent::Live { source: Source::Them, status: LiveStatus::Using(name) }) if name == "Deepgram"
    ));
    feed.push_silence(Duration::from_secs(2));
    until(&mut meeting, |e| said(e, "We're back.")).await;
    drop(feed);
    let (_, result) = finish(&mut meeting).await;
    assert_eq!(result.segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["We're back."]);
}

#[tokio::test]
async fn a_new_provider_replaces_only_sides_that_failed() {
    let failing = Line { fail: Some(ProviderErrorKind::Quota), ..Line::new("ElevenLabs", "") };
    let (feed, mut meeting) = start(failing);
    feed.push_silence(Duration::from_secs(2));
    until(&mut meeting, |e| matches!(e, EngineEvent::Error { .. })).await;
    let backup = Line::new("Deepgram", "Again.");
    let sides = backup.sides.clone();
    meeting.control().replace_live(Arc::new(backup));
    feed.push_silence(Duration::from_secs(2));
    until(&mut meeting, |e| said(e, "Again.")).await;
    assert_eq!(*sides.lock().unwrap(), [Source::Them], "Me never failed, so it keeps its provider");
    drop(feed);
}

#[tokio::test]
async fn reconnecting_and_back_are_told() {
    let live = Line { blips: true, ..Line::new("ElevenLabs", "Still here.") };
    let (feed, mut meeting) = start(live);
    feed.push_silence(Duration::from_secs(2));
    let events = until(&mut meeting, |e| said(e, "Still here.")).await;
    let statuses: Vec<&LiveStatus> = events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::Live { source: Source::Them, status } => Some(status),
            _ => None,
        })
        .collect();
    assert_eq!(statuses, [&LiveStatus::Using("ElevenLabs".into()), &LiveStatus::Reconnecting, &LiveStatus::Connected]);
    drop(feed);
}

#[tokio::test]
async fn both_sides_are_captioned_from_the_start() {
    let live = Line { after_ms: 500, ..Line::new("ElevenLabs", "Okay.") };
    let sides = live.sides.clone();
    let (feed, mut meeting) = start(live);
    feed.push_silence(Duration::from_secs(1));
    until(&mut meeting, |e| matches!(e, EngineEvent::Segment(s) if s.source == Source::Me && s.is_final)).await;
    assert_eq!(*sides.lock().unwrap(), [Source::Them, Source::Me]);
    drop(feed);
    let (_, result) = finish(&mut meeting).await;
    assert!(result.segments.iter().any(|s| s.source == Source::Me), "my line is kept for Notes");
}

async fn finish(meeting: &mut Meeting) -> (Vec<EngineEvent>, recap_engine::MeetingResult) {
    let mut events = until(meeting, |e| matches!(e, EngineEvent::Finished(_))).await;
    let Some(EngineEvent::Finished(result)) = events.pop() else { unreachable!() };
    (events, result)
}
