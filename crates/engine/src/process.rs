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

/// The first process of `exe` (by name, any case) whose parent is not `exe`
/// too: the app's root, whose tree holds its helpers.
pub fn app_root(list: &[Process], exe: &str) -> Option<u32> {
    let is_app = |p: &Process| p.exe.eq_ignore_ascii_case(exe);
    list.iter()
        .filter(|p| is_app(p))
        .find(|p| !list.iter().any(|q| q.pid == p.parent && is_app(q)))
        .map(|p| p.pid)
}

/// The processes running now; empty if they can't be listed.
#[cfg(windows)]
pub fn processes() -> Vec<Process> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
        },
    };
    let mut out = Vec::new();
    // SAFETY: a snapshot of ours, walked with a correctly sized entry and
    // closed once.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            out.push(Process {
                pid: entry.th32ProcessID,
                parent: entry.th32ParentProcessID,
                exe: String::from_utf16_lossy(&entry.szExeFile[..len]),
            });
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    out
}
