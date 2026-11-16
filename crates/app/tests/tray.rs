//! The tray icon's menu and picture (the OS part is drawn by `tray-icon`).

use recap::{
    commands::Command,
    tray::{TrayItem, icon_rgba, menu},
};

fn labels(items: &[TrayItem]) -> Vec<&str> {
    items.iter().map(|i| i.label).collect()
}

#[test]
fn the_menu_starts_or_stops_notes_opens_home_and_settings_and_quits() {
    let idle = menu(false);
    assert_eq!(labels(&idle), ["Start notes", "Open Home", "Settings", "", "Quit Recap"]);
    let commands: Vec<_> = idle.iter().map(|i| i.command).collect();
    assert_eq!(
        commands,
        [Some(Command::ToggleNotes), Some(Command::OpenHome), Some(Command::OpenSettings), None, Some(Command::Quit)]
    );
    assert_eq!(labels(&menu(true))[0], "Stop notes");
    assert_eq!(menu(true)[0].command, Some(Command::ToggleNotes));
}

#[test]
fn the_icon_is_a_ring_at_rest_and_a_red_dot_while_recording() {
    let size = 32;
    let px = |rgba: &[u8], x: usize, y: usize| -> [u8; 4] {
        let i = (y * size + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    let idle = icon_rgba(false, size as u32);
    assert_eq!(idle.len(), size * size * 4);
    assert_eq!(px(&idle, 16, 16)[3], 0, "a ring: its middle is empty");
    let ring = px(&idle, 16, 3);
    assert!(ring[3] > 200 && ring[0] > 200 && ring[1] > 200, "white ring {ring:?}");
    assert_eq!(px(&idle, 0, 0)[3], 0, "corners are clear");

    let recording = icon_rgba(true, size as u32);
    let dot = px(&recording, 16, 16);
    assert!(dot[3] > 200 && dot[0] > 200 && dot[1] < 120, "red middle {dot:?}");
}
