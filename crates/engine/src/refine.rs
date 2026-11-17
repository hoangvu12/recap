//! The refined transcript (GLOSSARY: Refined transcript): the two batch
//! transcripts of a meeting, Them diarized and Me alone, merged by time into
//! turns, with the echoes of Them on the mic dropped (the live rule,
//! [`crate::echo`]).
//!
//! Words become sentences first (a speaker change, a full stop, or a pause of
//! [`SENTENCE_GAP_MS`] ends one): the echo check compares sentences, as the
//! live meeting compares segments. The kept sentences, in time order, join
//! into turns while the same speaker goes on.

use serde::{Deserialize, Serialize};

use crate::{
    BatchTranscript, Source, Word,
    echo::{Spoken, is_echo},
    mic_levels::MicLevels,
};

/// Silence that ends a sentence even without punctuation.
pub const SENTENCE_GAP_MS: u64 = 1_500;

/// Me's speaker id in the refined transcript.
pub const ME: &str = "me";

/// One speaker's turn: `me`, or a diarized voice `S1..Sn` of Them. Times are
/// meeting time (the recording's), ms.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub source: Source,
    pub speaker: Option<String>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches(['"', '\'', ')', '”', '’']).ends_with(['.', '?', '!'])
}

/// The sentences of one side's words, each with its speaker.
fn sentences(words: &[Word], source: Source) -> Vec<Turn> {
    let speaker_of = |w: &Word| match source {
        Source::Me => ME.to_owned(),
        // Not diarized (a backup without it): one voice.
        Source::Them => w.speaker.clone().unwrap_or_else(|| "S1".to_owned()),
    };
    let mut out: Vec<Turn> = Vec::new();
    let mut open = false;
    for w in words.iter().filter(|w| !w.text.is_empty()) {
        let speaker = speaker_of(w);
        let joins = open
            && out.last().is_some_and(|last| {
                last.speaker.as_deref() == Some(speaker.as_str()) && w.start_ms <= last.end_ms + SENTENCE_GAP_MS
            });
        if joins {
            let last = out.last_mut().expect("an open sentence");
            last.text.push(' ');
            last.text.push_str(&w.text);
            last.end_ms = last.end_ms.max(w.end_ms);
        } else {
            out.push(Turn {
                source,
                speaker: Some(speaker),
                start_ms: w.start_ms,
                end_ms: w.end_ms,
                text: w.text.clone(),
            });
        }
        open = !ends_sentence(&w.text);
    }
    out
}

/// The refined transcript's turns from Me's words (`me`, not diarized) and
/// Them's (`them`, diarized `S1..`), in time order.
pub fn turns(me: &BatchTranscript, them: &BatchTranscript) -> Vec<Turn> {
    turns_with(me, them, None)
}

/// [`turns`], with the recording's `levels` judging echoes too (ADR 0021):
/// a Me sentence the mic only heard Them say back is dropped, whatever its
/// words.
pub fn turns_with(me: &BatchTranscript, them: &BatchTranscript, levels: Option<&MicLevels>) -> Vec<Turn> {
    let theirs = sentences(&them.words, Source::Them);
    let spoken: Vec<Spoken<'_>> =
        theirs.iter().map(|t| Spoken { start_ms: t.start_ms, end_ms: t.end_ms, text: &t.text }).collect();
    let mine = sentences(&me.words, Source::Me).into_iter().filter(|s| {
        !levels.is_some_and(|l| l.is_echo(s.start_ms, s.end_ms))
            && !is_echo(Spoken { start_ms: s.start_ms, end_ms: s.end_ms, text: &s.text }, &spoken)
    });
    let mut all: Vec<Turn> = theirs.iter().cloned().chain(mine).collect();
    // Them first on a tie: what the mic heard at the same moment follows.
    all.sort_by_key(|s| (s.start_ms, s.source == Source::Me));

    let mut out: Vec<Turn> = Vec::new();
    for s in all {
        match out.last_mut() {
            Some(last) if last.source == s.source && last.speaker == s.speaker => {
                last.text.push(' ');
                last.text.push_str(&s.text);
                last.end_ms = last.end_ms.max(s.end_ms);
            }
            _ => out.push(s),
        }
    }
    out
}
