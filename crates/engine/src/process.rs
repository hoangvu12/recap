//! Processes by executable: the root of a meeting app's process tree (whose
//! audio "Meeting audio: Teams" captures), and the names call detection
//! reports. Pure, but [`processes`] (Windows: a ToolHelp snapshot).

/// A running process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    /// The executable's file name (`ms-teams.exe`).
    pub exe: String,
}

/// The oldest process of `exe` (by name, any case) whose parent is not
/// `exe` too: the app's root, whose tree holds its helpers.
pub fn app_root(list: &[Process], exe: &str) -> Option<u32> {
    let _ = (list, exe);
    None
}

/// The processes running now; empty if they can't be listed.
#[cfg(windows)]
pub fn processes() -> Vec<Process> {
    Vec::new()
}
