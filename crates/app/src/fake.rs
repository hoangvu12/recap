//! A meeting without audio: the test script (testdata/meeting_tts_script.txt)
//! spoken in real time by the real engine, through a scripted provider and
//! silence. Until the audio tickets land, the app runs this.

use std::sync::Arc;

use recap_engine::{
    EngineEvent, Meeting, MeetingConfig, MeetingControl, Script, ScriptedLiveStt, SilenceSource,
};
use tokio::{runtime::Handle, sync::mpsc::UnboundedReceiver};

/// Start it on `runtime`; its events and its controls.
pub fn start(runtime: &Handle) -> (UnboundedReceiver<EngineEvent>, MeetingControl) {
    let _inside = runtime.enter();
    Meeting::start(
        MeetingConfig { caption_me: true, ..Default::default() },
        Box::new(SilenceSource::paced(None)),
        Arc::new(ScriptedLiveStt::new(Script::test_meeting())),
    )
    .split()
}
