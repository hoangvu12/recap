//! Open Recap when Windows starts (Settings, General): a value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, written when the
//! switch is on and removed when it is off. Recap started that way stays
//! in the tray (`--background`).

use std::path::Path;

use crate::user_settings::SettingsHost;

/// The Run key Windows reads at sign-in.
pub const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// What the Run value starts: this executable, in the tray.
pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" --background", exe.display())
}

/// One value (`name`) under a Run key of HKCU (`subkey`).
#[derive(Clone, Debug)]
pub struct RunKey {
    subkey: String,
    name: String,
}

impl RunKey {
    pub fn new(subkey: &str, name: &str) -> Self {
        Self { subkey: subkey.to_owned(), name: name.to_owned() }
    }

    /// Recap's value in the real Run key.
    pub fn recap() -> Self {
        Self::new(RUN, "Recap")
    }

    /// The command it holds, if any.
    #[cfg(windows)]
    pub fn get(&self) -> Option<String> {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        RegKey::predef(HKEY_CURRENT_USER).open_subkey(&self.subkey).ok()?.get_value(&self.name).ok()
    }

    /// Write `command`, or remove the value (`None`).
    #[cfg(windows)]
    pub fn set(&self, command: Option<&str>) -> std::io::Result<()> {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match command {
            Some(command) => hkcu.create_subkey(&self.subkey)?.0.set_value(&self.name, &command.to_string()),
            None => match hkcu.open_subkey_with_flags(&self.subkey, winreg::enums::KEY_SET_VALUE) {
                Ok(key) => match key.delete_value(&self.name) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                    _ => Ok(()),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            },
        }
    }

    /// Delete the subkey's parent and what it holds, then its grandparent
    /// if that is left empty (tests' own keys only; never the Run key).
    #[cfg(windows)]
    pub fn delete_key(&self) {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        assert!(self.subkey.to_ascii_lowercase().contains("recap-test"), "only a test's own key");
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Some((parent, _)) = self.subkey.rsplit_once('\\') else { return };
        let _ = hkcu.delete_subkey_all(parent);
        if let Some((grandparent, _)) = parent.rsplit_once('\\') {
            let _ = hkcu.delete_subkey(grandparent); // only when empty
        }
    }

    #[cfg(not(windows))]
    pub fn get(&self) -> Option<String> {
        None
    }

    #[cfg(not(windows))]
    pub fn set(&self, _command: Option<&str>) -> std::io::Result<()> {
        Ok(())
    }
}

/// Keep `key` as the autostart setting says: now and on each change. When
/// on, it starts `command`; off, Recap's value is removed.
pub fn connect(settings: &SettingsHost, key: RunKey, command: String) {
    let apply = move |on: bool| {
        let want = on.then_some(command.as_str());
        if key.get().as_deref() != want
            && let Err(error) = key.set(want)
        {
            eprintln!("recap: autostart: {error}");
        }
    };
    apply(settings.current().autostart);
    settings.on_change(move |old, new| {
        if old.autostart != new.autostart {
            apply(new.autostart);
        }
    });
}
