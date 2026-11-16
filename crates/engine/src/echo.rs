//! Echo dedup (GLOSSARY: Echo). On laptop speakers the mic hears the meeting
//! audio, so what Them says comes back as Me. A Me segment is an echo when
//! Them said the same words (similarity ≥ [`ECHO_SIMILARITY`]) within
//! [`ECHO_WINDOW_MS`] of it. The live session drops echoes before they are
//! shown or saved; the after-Stop transcript uses the same rule.

/// How far apart (ms, between the two segments) an echo can be.
pub const ECHO_WINDOW_MS: u64 = 1_750;
/// How much of the words must match.
pub const ECHO_SIMILARITY: f64 = 0.85;

/// Below this many words, a Me segment must match what Them said as a whole,
/// not just appear in it ("Yes." is in many sentences).
const SHORT: usize = 3;

/// Something said, on meeting time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spoken<'a> {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: &'a str,
}

/// The words of `text`: lowercase letters and digits, punctuation dropped.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect()
}

/// The longest common subsequence of two word lists, in words.
fn lcs(a: &[String], b: &[String]) -> usize {
    let mut row = vec![0usize; b.len() + 1];
    for x in a {
        let mut diagonal = 0;
        for (j, y) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = if x == y { diagonal + 1 } else { up.max(row[j]) };
            diagonal = up;
        }
    }
    row[b.len()]
}

fn similarity_of(mic: &[String], them: &[String]) -> f64 {
    if mic.is_empty() || them.is_empty() {
        return 0.0;
    }
    let common = lcs(mic, them) as f64;
    if mic.len() < SHORT {
        2.0 * common / (mic.len() + them.len()) as f64
    } else {
        // The mic may hear only part of a turn: how much of it Them said.
        common / mic.len() as f64
    }
}

/// How much of what the mic heard (`mic`) is what Them said (`them`), 0–1,
/// by words in order.
pub fn similarity(mic: &str, them: &str) -> f64 {
    similarity_of(&words(mic), &words(them))
}

/// Whether `me` is an echo of what Them said (`them`, any order).
pub fn is_echo(me: Spoken<'_>, them: &[Spoken<'_>]) -> bool {
    let mut near: Vec<&Spoken<'_>> = them
        .iter()
        .filter(|t| t.start_ms <= me.end_ms + ECHO_WINDOW_MS && me.start_ms <= t.end_ms + ECHO_WINDOW_MS)
        .collect();
    if near.is_empty() {
        return false;
    }
    near.sort_by_key(|t| t.start_ms);
    let mic = words(me.text);
    // Each segment alone (a short reply must match one whole), then all of
    // them in order (a turn the two sides cut differently).
    let all: Vec<String> = near.iter().flat_map(|t| words(t.text)).collect();
    near.iter().any(|t| similarity_of(&mic, &words(t.text)) >= ECHO_SIMILARITY)
        || similarity_of(&mic, &all) >= ECHO_SIMILARITY
}
