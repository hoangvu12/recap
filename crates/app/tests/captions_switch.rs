//! A live meeting whose captions job falls back, through the real session
//! and host: the UI's `meeting` says which provider captions now and what
//! "Switch to …" would move to, and the user can switch.

use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

use recap::{
    host::{self, Host},
    live::{LiveCaptions, switch_notice},
    session::{self, SessionConfig},
};
use recap_engine::{
    AudioFrame, BoxFuture, Capabilities, Fallback, FrameFeed, LiveEvent, LiveOptions, LiveStt, ProviderError,
    ProviderErrorKind, Source,
};
use tokio::sync::mpsc;
use zgx_script::{Value, testing::TestApp};

/// Captions each frame as a final ("<name> <start_ms>"); gives up out of
/// quota once it has heard `quota_after` frames.
struct FakeLive {
    name: &'static str,
    quota_after: Option<usize>,
}

impl LiveStt for FakeLive {
    fn name(&self) -> &str {
        self.name
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { live: true, ..Default::default() }
    }
    fn run(
        &self,
        _source: Source,
        _options: LiveOptions,
        mut audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let (name, quota_after) = (self.name, self.quota_after);
        Box::pin(async move {
            let mut heard = 0;
            while let Some(frame) = audio.recv().await {
                if quota_after == Some(heard) {
                    return Err(ProviderError::new(ProviderErrorKind::Quota, format!("{name} quota: out of credit")));
                }
                heard += 1;
                let text = format!("{name} {}", frame.start_ms);
                let _ = events.send(LiveEvent { text, is_final: true, start_ms: frame.start_ms, end_ms: frame.start_ms + 100 });
            }
            Ok(())
        })
    }
}

fn job(quota_after: Option<usize>) -> Fallback<dyn LiveStt> {
    Fallback::new(
        Arc::new(FakeLive { name: "ElevenLabs", quota_after }) as Arc<dyn LiveStt>,
        Some(Arc::new(FakeLive { name: "Deepgram", quota_after: None }) as Arc<dyn LiveStt>),
    )
}

struct Fixture {
    runtime: tokio::runtime::Runtime,
    _app: TestApp,
    host: Host,
}

impl Fixture {
    fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        let made: Rc<RefCell<Option<Host>>> = Rc::default();
        let out = made.clone();
        let app = TestApp::new(move |rt| {
            let host = Host::new(rt, host::script_path(), None);
            *out.borrow_mut() = Some(host.clone());
            host.script.clone()
        });
        app.frames(2);
        let host = made.borrow_mut().take().unwrap();
        Fixture { runtime, _app: app, host }
    }

    /// The UI's `meeting`.
    fn meeting(&self) -> serde_json::Value {
        let shared = self.host.script.shared("meeting", Value::Nil);
        serde_json::from_str(&shared.get().to_json()).unwrap()
    }

    /// A meeting of `seconds` of audio captioned by `job`, followed by the
    /// host; `during` runs once it has started.
    fn meeting_with(&self, job: Fallback<dyn LiveStt>, seconds: u64, during: impl FnOnce(&Self, &FrameFeed)) {
        let (notify, notices) = session::notices();
        let captions = LiveCaptions::job(job.on_switch(move |n| notify(switch_notice(&n))));
        let config = SessionConfig { started_ms: 1, caption_me: false, live: LiveOptions::default() };
        let (feed, source) = FrameFeed::new();
        let started = {
            let _inside = self.runtime.enter();
            session::start(None, config.clone(), Box::new(source), captions.stt, Some(notices))
        };
        let follow = self.host.follow_session(&config, started, captions.switch);
        during(self, &feed);
        feed.push_silence(Duration::from_secs(seconds));
        drop(feed);
        self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(20), follow).await.expect("the meeting ends");
        });
    }
}

#[test]
fn when_the_main_runs_out_of_quota_the_ui_shows_the_backup_captioning() {
    let f = Fixture::new();
    f.meeting_with(job(Some(3)), 2, |f, _| {
        let m = f.meeting();
        assert_eq!((m["captions_by"].as_str(), m["switch_to"].as_str()), (Some("ElevenLabs"), Some("Deepgram")));
    });
    let m = f.meeting();
    assert_eq!(m["captions_by"], "Deepgram", "{m}");
    assert_eq!(m["switch_to"], "", "nothing after the backup, and the meeting is over");
    let notice = m["notice"].as_str().unwrap();
    assert!(notice.starts_with("ElevenLabs failed (ElevenLabs quota: out of credit); switched to Deepgram"), "{notice}");
}

#[test]
fn the_user_can_switch_captions_to_the_backup() {
    let f = Fixture::new();
    f.meeting_with(job(None), 1, |f, feed| {
        feed.push_silence(Duration::from_millis(500));
        f.host.switch_captions();
        let m = f.meeting();
        assert_eq!((m["captions_by"].as_str(), m["switch_to"].as_str()), (Some("Deepgram"), Some("")));
        assert_eq!(m["notice"], "Switched to Deepgram");
    });
    assert_eq!(f.meeting()["captions_by"], "Deepgram");
}

#[test]
fn a_lone_provider_has_nothing_to_switch_to() {
    let f = Fixture::new();
    let (_, source) = FrameFeed::new();
    let config = SessionConfig::default();
    let started = {
        let _inside = f.runtime.enter();
        let stt: Arc<dyn LiveStt> = Arc::new(FakeLive { name: "Script", quota_after: None });
        session::start(None, config.clone(), Box::new(source), stt, None)
    };
    let _follow = f.host.follow_session(&config, started, None);
    f.host.switch_captions();
    let m = f.meeting();
    assert_eq!((m["captions_by"].as_str(), m["switch_to"].as_str()), (Some(""), Some("")));
    assert_eq!(m["notice"], "");
}
