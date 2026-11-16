//! The generated Luau design modules match docs/design, their source of
//! truth. When one fails, run `python tools/gen_design.py`.

use std::{fs, path::Path};

fn read(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}")).replace('\r', "")
}

/// The body of `M.<table> = { ... }` in a generated module.
fn table<'a>(luau: &'a str, name: &str) -> &'a str {
    let start = luau.find(&format!("M.{name} = {{")).unwrap_or_else(|| panic!("no M.{name}"));
    let body = &luau[start..];
    &body[..body.find("\n}").unwrap()]
}

#[test]
fn every_design_token_is_in_both_themes() {
    let json: serde_json::Value = serde_json::from_str(&read("docs/design/tokens.json")).unwrap();
    let luau = read("crates/app/ui/theme/tokens.luau");
    let (dark, light) = (table(&luau, "dark"), table(&luau, "light"));
    let tokens = json["tokens"].as_object().unwrap();
    assert!(tokens.len() > 20);
    for (name, token) in tokens {
        let key = format!("[\"rc.{}\"]", name.replace('-', "."));
        let line = |value: &serde_json::Value| format!("{key} = \"{}\",", value.as_str().unwrap());
        match &token["value"] {
            serde_json::Value::Array(modes) => {
                for mode in modes {
                    let theme = if mode["theme"]["mode"] == "dark" { dark } else { light };
                    assert!(theme.contains(&line(&mode["value"])), "{name} ({}) is stale", mode["theme"]["mode"]);
                }
            }
            value => {
                assert!(dark.contains(&line(value)) && light.contains(&line(value)), "{name} is stale");
            }
        }
    }
    let lines = |t: &str| t.lines().filter(|l| l.contains("[\"rc.")).count();
    assert_eq!(lines(dark), tokens.len(), "tokens.luau has tokens tokens.json lacks");
}

#[test]
fn every_provider_logo_is_drawn_from_the_logo_board() {
    let board = read("docs/design/screens.html");
    let luau = read("crates/app/ui/lib/logos.luau");
    let logos: Vec<&str> = luau.lines().filter(|l| l.trim_start().starts_with("[\"")).collect();
    assert_eq!(logos.len(), 14);
    for logo in &logos {
        for path in logo.split("d=\"").skip(1) {
            let d = &path[..path.find('"').unwrap()];
            assert!(board.contains(d), "a logo path is not on the board: {logo}");
        }
    }
    // Every preset's logo is there.
    for preset in recap_providers::Registry::builtin().presets() {
        let key = format!("[\"{}\"]", preset.logo);
        assert!(logos.iter().any(|l| l.contains(&key)), "{} has no logo {}", preset.id, preset.logo);
    }
}

#[test]
fn every_icon_is_drawn_from_the_icons_board() {
    let board = read("docs/design/screens.html");
    let luau = read("crates/app/ui/lib/icons.luau");
    let icons: Vec<&str> = luau.lines().filter(|l| l.trim_start().starts_with("[\"")).collect();
    assert_eq!(icons.len(), 47);
    for icon in icons {
        for path in icon.split("d=\"").skip(1) {
            let d = &path[..path.find('"').unwrap()];
            assert!(board.contains(d), "an icon path is not on the board: {icon}");
        }
    }
}
