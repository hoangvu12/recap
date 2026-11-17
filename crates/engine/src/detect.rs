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

/// The apps a call happens in, by executable.
const APPS: [(&str, &str); 14] = [
    ("ms-teams.exe", "Teams"),
    ("teams.exe", "Teams"),
    ("zoom.exe", "Zoom"),
    ("webex.exe", "Webex"),
    ("ciscocollabhost.exe", "Webex"),
    ("slack.exe", "Slack"),
    ("discord.exe", "Discord"),
    ("chrome.exe", "Chrome"),
    ("msedge.exe", "Edge"),
    ("brave.exe", "Brave"),
    ("firefox.exe", "Firefox"),
    ("opera.exe", "Opera"),
    ("vivaldi.exe", "Vivaldi"),
    ("arc.exe", "Arc"),
];

/// A meeting app by its executable (a file name or a path, any case): its
/// name as a toast says it.
pub fn meeting_app(exe: &str) -> Option<&'static str> {
    let name = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    APPS.iter().find(|(e, _)| e.eq_ignore_ascii_case(name)).map(|(_, app)| *app)
}

/// The executable a ConsentStore key names: `C:#Program Files#Zoom#bin#Zoom.exe`
/// (an unpackaged app's path), or a package family (`MSTeams_8wekyb3d8bbwe`).
pub fn consent_exe(key: &str) -> Option<String> {
    if key.contains('#') {
        return key.rsplit('#').next().filter(|s| !s.is_empty()).map(str::to_ascii_lowercase);
    }
    let family = key.split('_').next()?;
    match family.to_ascii_lowercase().as_str() {
        "msteams" => Some("ms-teams.exe".into()),
        _ => None,
    }
}

/// Hints from what holds the mic, second by second.
#[derive(Debug, Default)]
pub struct CallWatch {
    /// The meeting apps on the mic at the last look.
    on_mic: Vec<&'static str>,
    /// The app the live meeting's call is in, once seen on the mic.
    call: Option<&'static str>,
    /// Since when the mic has been free during notes.
    free_since: Option<u64>,
    offered_stop: bool,
}

impl CallWatch {
    /// `users` (executables) hold the mic at `now_ms`; `live` whether notes
    /// are being taken. A hint when one is due.
    pub fn observe(&mut self, now_ms: u64, users: &[String], live: bool) -> Option<CallHint> {
        let mut apps: Vec<&'static str> = Vec::new();
        for app in users.iter().filter_map(|u| meeting_app(u)) {
            if !apps.contains(&app) {
                apps.push(app);
            }
        }
        let new = apps.iter().find(|a| !self.on_mic.contains(a)).copied();
        self.on_mic = apps;
        if !live {
            self.call = None;
            self.free_since = None;
            self.offered_stop = false;
            return new.map(|app| CallHint::Started { app });
        }
        if let Some(first) = self.on_mic.first() {
            if !self.call.is_some_and(|c| self.on_mic.contains(&c)) {
                self.call = Some(*first);
            }
            self.free_since = None;
            self.offered_stop = false;
            return None;
        }
        let app = self.call?;
        let since = *self.free_since.get_or_insert(now_ms);
        if !self.offered_stop && now_ms.saturating_sub(since) >= RELEASED_FOR_MS {
            self.offered_stop = true;
            return Some(CallHint::Released { app });
        }
        None
    }
}

/// The executables holding a microphone now (lower case, not this process):
/// active capture sessions, and apps the ConsentStore says use the mic while
/// any other session is active (a crash can leave its entry "in use").
/// Call it on a thread with COM set up (`wasapi::initialize_mta`).
#[cfg(windows)]
pub fn mic_users() -> Vec<String> {
    use wasapi::{DeviceEnumerator, Direction, SessionState};
    let me = std::process::id();
    let names: std::collections::HashMap<u32, String> =
        crate::process::processes().into_iter().map(|p| (p.pid, p.exe.to_ascii_lowercase())).collect();
    let mut users: Vec<String> = Vec::new();
    fn add(exe: String, users: &mut Vec<String>) {
        if !users.contains(&exe) {
            users.push(exe);
        }
    }
    if let Ok(devices) = DeviceEnumerator::new().and_then(|e| e.get_device_collection(Direction::Capture)) {
        for i in 0..devices.get_nbr_devices().unwrap_or(0) {
            let Ok(sessions) = devices
                .get_device_at_index(i)
                .and_then(|d| d.get_iaudiosessionmanager())
                .and_then(|m| m.get_audiosessionenumerator())
            else {
                continue;
            };
            for s in 0..sessions.get_count().unwrap_or(0) {
                let Ok(session) = sessions.get_session(s) else { continue };
                if !matches!(session.get_state(), Ok(SessionState::Active)) {
                    continue;
                }
                let pid = session.get_process_id().unwrap_or(0);
                if pid == 0 || pid == me {
                    continue;
                }
                if let Some(exe) = names.get(&pid) {
                    add(exe.clone(), &mut users);
                }
            }
        }
    }
    if !users.is_empty() {
        let own =
            std::env::current_exe().ok().and_then(|p| Some(p.file_name()?.to_string_lossy().to_ascii_lowercase()));
        for exe in consent_in_use() {
            if Some(&exe) != own.as_ref() {
                add(exe, &mut users);
            }
        }
    }
    users
}

/// [`mic_users`] every second while `enabled` says to look, on a thread of
/// its own (COM there is ours). It ends once the receiver is dropped.
#[cfg(windows)]
pub fn poll_mic_users(
    enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> tokio::sync::mpsc::UnboundedReceiver<Vec<String>> {
    use std::sync::atomic::Ordering;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let _ = std::thread::Builder::new().name("recap-call-watch".into()).spawn(move || {
        let _com = wasapi::initialize_mta();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if tx.is_closed() {
                return;
            }
            if enabled.load(Ordering::Relaxed) && tx.send(mic_users()).is_err() {
                return;
            }
        }
    });
    rx
}

/// The ConsentStore's apps using the microphone now (`LastUsedTimeStop` 0).
#[cfg(windows)]
fn consent_in_use() -> Vec<String> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    const MIC: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
    let in_use = |key: &RegKey| {
        let start: u64 = key.get_value("LastUsedTimeStart").unwrap_or(0);
        let stop: u64 = key.get_value("LastUsedTimeStop").unwrap_or(1);
        start != 0 && stop == 0
    };
    let mut out = Vec::new();
    let Ok(root) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(MIC) else { return out };
    let mut scan = |parent: &RegKey| {
        for name in parent.enum_keys().flatten() {
            if name == "NonPackaged" {
                continue;
            }
            if let Ok(key) = parent.open_subkey(&name)
                && in_use(&key)
                && let Some(exe) = consent_exe(&name)
            {
                out.push(exe);
            }
        }
    };
    scan(&root);
    if let Ok(unpackaged) = root.open_subkey("NonPackaged") {
        scan(&unpackaged);
    }
    out
}
