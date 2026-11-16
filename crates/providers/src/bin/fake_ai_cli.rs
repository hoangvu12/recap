//! A stand-in AI command line for the `cli` provider's tests. Not shipped.
//!
//! - `fake_ai_cli env [args…] [--file <path>]`: prints, as JSON, what it was
//!   run with: its args, stdin, the file's text, how many entries its
//!   working folder has, and whether `ANTHROPIC_API_KEY` /
//!   `ANTHROPIC_AUTH_TOKEN` reached it.
//! - `fake_ai_cli seq <dir>`: run number N (counted in `<dir>/count`) prints
//!   `<dir>/N.out` to stdout and `<dir>/N.err` to stderr, and exits with
//!   `<dir>/N.code` (0 when missing).

use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("env") => env(&args[1..]),
        Some("seq") => seq(&args[1]),
        _ => {
            eprintln!("usage: fake_ai_cli env|seq …");
            std::process::exit(2);
        }
    }
}

fn env(args: &[String]) {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let file = args
        .iter()
        .position(|a| a == "--file")
        .and_then(|at| args.get(at + 1))
        .map(|path| std::fs::read_to_string(path).unwrap_or_else(|e| format!("unreadable: {e}")));
    let cwd_entries = std::fs::read_dir(".").map(|d| d.count()).unwrap_or(usize::MAX);
    let out = serde_json::json!({
        "args": args,
        "stdin": stdin,
        "file": file,
        "cwd_entries": cwd_entries,
        "anthropic_api_key": std::env::var_os("ANTHROPIC_API_KEY").is_some(),
        "anthropic_auth_token": std::env::var_os("ANTHROPIC_AUTH_TOKEN").is_some(),
    });
    println!("{out}");
}

fn seq(dir: &str) {
    let dir = std::path::Path::new(dir);
    let count = std::fs::read_to_string(dir.join("count")).ok().and_then(|c| c.trim().parse::<u32>().ok()).unwrap_or(0) + 1;
    std::fs::write(dir.join("count"), count.to_string()).unwrap();
    if let Ok(out) = std::fs::read_to_string(dir.join(format!("{count}.out"))) {
        print!("{out}");
    }
    if let Ok(err) = std::fs::read_to_string(dir.join(format!("{count}.err"))) {
        eprint!("{err}");
    }
    let code = std::fs::read_to_string(dir.join(format!("{count}.code"))).ok().and_then(|c| c.trim().parse().ok()).unwrap_or(0);
    std::process::exit(code);
}
