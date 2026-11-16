//! Call detection (ticket #15): which meeting app holds the mic, polled
//! each second ([`mic_users`] on Windows), turned into hints: a call began
//! (offer Start notes) or, during notes, the mic has been free for a minute
//! (offer Stop). A hint only suggests; nothing here starts or stops.

/// How long the mic stays free during notes before Stop is offered.
pub const RELEASED_FOR_MS: u64 = 60_000;

/// What a toast suggests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallHint {
    /// `app` took the mic: Start notes?
    Started { app: &'static str },
    /// The call in `app` left the mic a minute ago: Stop notes?
    Released { app: &'static str },
}

/// A meeting app by its executable (a file name or a path, any case): its
/// name as a toast says it.
pub fn meeting_app(exe: &str) -> Option<&'static str> {
    let _ = exe;
    None
}

/// The executable a ConsentStore key names: `C:#Program Files#Zoom#bin#Zoom.exe`
/// (an unpackaged app's path), or a package family (`MSTeams_8wekyb3d8bbwe`).
pub fn consent_exe(key: &str) -> Option<String> {
    let _ = key;
    None
}

/// Hints from what holds the mic, second by second.
#[derive(Debug, Default)]
pub struct CallWatch {}

impl CallWatch {
    /// `users` (executables) hold the mic at `now_ms`; `live` whether notes
    /// are being taken. A hint when one is due.
    pub fn observe(&mut self, now_ms: u64, users: &[String], live: bool) -> Option<CallHint> {
        let _ = (now_ms, users, live);
        None
    }
}
