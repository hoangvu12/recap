//! The tray icon (ticket #15): where Recap lives. Its menu starts or stops
//! notes, opens Home and Settings, and quits; a click on the icon opens
//! Home. The icon is a ring at rest and a red dot while recording.
//!
//! [`menu`] and [`icon_rgba`] are the model; [`Tray`] (Windows, through
//! `tray-icon`) draws it. Make the tray on the UI thread.

use crate::commands::Command;

/// A menu item; a separator has no command (and no label).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrayItem {
    pub label: &'static str,
    pub command: Option<Command>,
}

/// The menu, as it is while notes are `live` or not.
pub fn menu(live: bool) -> Vec<TrayItem> {
    let item = |label, command| TrayItem { label, command: Some(command) };
    vec![
        item(if live { "Stop notes" } else { "Start notes" }, Command::ToggleNotes),
        item("Open Home", Command::OpenHome),
        item("Settings", Command::OpenSettings),
        TrayItem { label: "", command: None },
        item("Quit Recap", Command::Quit),
    ]
}

/// The icon, `size` px square, RGBA: a white ring, or the rec red dot.
pub fn icon_rgba(recording: bool, size: u32) -> Vec<u8> {
    let s = size as f32;
    let center = s / 2.;
    let outer = s * 0.44;
    // The ring's width; the dot fills to the outer edge.
    let inner = if recording { 0. } else { outer - (s * 0.12).max(1.5) };
    // Rec red (tokens: $rc.rec), white.
    let (r, g, b) = if recording { (0xFF, 0x63, 0x63) } else { (0xFF, 0xFF, 0xFF) };
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - center).powi(2) + (y as f32 + 0.5 - center).powi(2)).sqrt();
            // Coverage, antialiased over a pixel at both edges.
            let cover = (outer - d + 0.5).clamp(0., 1.) * if inner > 0. { (d - inner + 0.5).clamp(0., 1.) } else { 1. };
            out.extend_from_slice(&[r, g, b, (cover * 255.).round() as u8]);
        }
    }
    out
}

#[cfg(windows)]
pub use system::Tray;

#[cfg(windows)]
mod system {
    use super::*;
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    };

    const SIZE: u32 = 32;

    /// The tray icon and its menu.
    pub struct Tray {
        icon: TrayIcon,
        toggle: MenuItem,
        live: bool,
    }

    fn icon(recording: bool) -> Option<Icon> {
        Icon::from_rgba(icon_rgba(recording, SIZE), SIZE, SIZE).ok()
    }

    fn id(command: Command) -> String {
        format!("{command:?}")
    }

    impl Tray {
        /// The icon in the tray, and its commands as they are chosen (a
        /// left click on the icon is Open Home). Call once.
        pub fn new() -> Result<(Tray, tokio::sync::mpsc::UnboundedReceiver<Command>), String> {
            let menu = Menu::new();
            let mut toggle = None;
            for item in super::menu(false) {
                match item.command {
                    None => menu.append(&PredefinedMenuItem::separator()),
                    Some(command) => {
                        let entry = MenuItem::with_id(id(command), item.label, true, None);
                        let added = menu.append(&entry);
                        if command == Command::ToggleNotes {
                            toggle = Some(entry);
                        }
                        added
                    }
                }
                .map_err(|e| e.to_string())?;
            }
            let icon = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .with_tooltip("Recap")
                .with_icon(icon(false).ok_or("no icon")?)
                .build()
                .map_err(|e| e.to_string())?;

            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let commands = super::menu(true).into_iter().chain(super::menu(false)).filter_map(|i| i.command);
            let by_id: Vec<(String, Command)> = commands.map(|c| (id(c), c)).collect();
            let menu_tx = tx.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if let Some((_, command)) = by_id.iter().find(|(id, _)| *id == event.id.0) {
                    let _ = menu_tx.send(*command);
                }
            }));
            TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    let _ = tx.send(Command::OpenHome);
                }
            }));
            Ok((Tray { icon, toggle: toggle.ok_or("no Start item")?, live: false }, rx))
        }

        /// Show notes as `live` (the first item and the icon).
        pub fn set_live(&mut self, live: bool) {
            if live == self.live {
                return;
            }
            self.live = live;
            self.toggle.set_text(super::menu(live)[0].label);
            let _ = self.icon.set_icon(icon(live));
            let _ = self.icon.set_tooltip(Some(if live { "Recap: taking notes" } else { "Recap" }));
        }
    }
}
