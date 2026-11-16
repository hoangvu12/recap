//! Storage-free fakes for tests and for running the app without providers.

use tokio::sync::mpsc;

use crate::{
    AudioFrame, BoxFuture, Capabilities, LiveEvent, LiveOptions, LiveStt, ProviderError, Source,
};

/// One spoken line of a meeting script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptLine {
    /// The TTS voice that says it (`Mark`, `Zira`, `David`).
    pub voice: String,
    pub source: Source,
    pub text: String,
}

/// A meeting script as `testdata/meeting_tts_script.txt` writes it: one
/// `Voice|text` line per turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub lines: Vec<ScriptLine>,
}

impl Script {
    /// Parse `text`; lines said by `me_voice` are Me (the mic), the rest Them.
    pub fn parse(text: &str, me_voice: &str) -> Result<Self, String> {
        let mut lines = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (voice, said) =
                line.split_once('|').ok_or_else(|| format!("line {}: no `Voice|text`", n + 1))?;
            let source = if voice == me_voice { Source::Me } else { Source::Them };
            lines.push(ScriptLine { voice: voice.to_owned(), source, text: said.trim().to_owned() });
        }
        Ok(Self { lines })
    }

    /// The test meeting (Tom = Me, Priya and David = Them).
    pub fn test_meeting() -> Self {
        Self::parse(include_str!("../../../testdata/meeting_tts_script.txt"), "Mark")
            .expect("the test meeting script parses")
    }
}

/// How fast the scripted speakers talk.
#[derive(Clone, Copy, Debug)]
pub struct Pace {
    /// Each word adds to the partial this long after the last.
    pub word_ms: u64,
    /// A sentence turns final this long after its last word.
    pub final_lag_ms: u64,
    /// Between sentences of one line.
    pub sentence_gap_ms: u64,
    /// Between lines (turns).
    pub line_gap_ms: u64,
}

impl Default for Pace {
    fn default() -> Self {
        // About 168 words a minute, a brisk meeting.
        Self { word_ms: 357, final_lag_ms: 200, sentence_gap_ms: 100, line_gap_ms: 700 }
    }
}

/// A live STT that hears nothing and replays a script instead: each sentence
/// grows word by word as partials, then turns final. It is clocked by the
/// audio it receives, so pausing the audio pauses it, and an unpaced source
/// replays a whole meeting at once.
pub struct ScriptedLiveStt {
    script: Script,
    pace: Pace,
}

impl ScriptedLiveStt {
    pub fn new(script: Script) -> Self {
        Self { script, pace: Pace::default() }
    }

    pub fn with_pace(mut self, pace: Pace) -> Self {
        self.pace = pace;
        self
    }

    /// Every event of the meeting with the time it is heard, in order; both
    /// sides share one timeline.
    fn schedule(&self) -> Vec<(u64, Source, LiveEvent)> {
        let p = self.pace;
        let mut out = Vec::new();
        let mut t = 0;
        for line in &self.script.lines {
            for sentence in sentences(&line.text) {
                let words: Vec<&str> = sentence.split(' ').collect();
                let start = t;
                for i in 1..=words.len() {
                    let at = start + i as u64 * p.word_ms;
                    let text = words[..i].join(" ");
                    out.push((at, line.source, LiveEvent { text, is_final: false, start_ms: start, end_ms: at }));
                }
                let done = start + words.len() as u64 * p.word_ms + p.final_lag_ms;
                out.push((
                    done,
                    line.source,
                    LiveEvent { text: sentence.to_owned(), is_final: true, start_ms: start, end_ms: done },
                ));
                t = done + p.sentence_gap_ms;
            }
            t += p.line_gap_ms - p.sentence_gap_ms;
        }
        out
    }
}

/// `text` split after each `.`, `?` or `!` that ends a word.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    let bytes = text.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if matches!(b, b'.' | b'?' | b'!') && bytes.get(i + 1) == Some(&b' ') {
            out.push(text[from..=i].trim());
            from = i + 1;
        }
    }
    let rest = text[from..].trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

impl LiveStt for ScriptedLiveStt {
    fn name(&self) -> &str {
        "Scripted"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { live: true, word_timestamps: false, ..Capabilities::default() }
    }

    fn run(
        &self,
        source: Source,
        _options: LiveOptions,
        mut audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let schedule: Vec<LiveEvent> = self
            .schedule()
            .into_iter()
            .filter(|(_, side, _)| *side == source)
            .map(|(at, _, event)| LiveEvent { end_ms: at.max(event.end_ms), ..event })
            .collect();
        Box::pin(async move {
            let mut heard_ms = 0;
            let mut next = 0;
            let mut partial: Option<LiveEvent> = None;
            while let Some(frame) = audio.recv().await {
                heard_ms += frame.duration_ms();
                while let Some(event) = schedule.get(next).filter(|e| e.end_ms <= heard_ms) {
                    partial = (!event.is_final).then(|| event.clone());
                    if events.send(event.clone()).is_err() {
                        return Ok(());
                    }
                    next += 1;
                }
            }
            // The audio closed mid-sentence: what was heard is final.
            if let Some(last) = partial {
                let _ = events.send(LiveEvent { is_final: true, ..last });
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_split_at_their_ends_only() {
        assert_eq!(
            sentences("Okay, let's go. Thanks, Tom. Who owns it? Me"),
            ["Okay, let's go.", "Thanks, Tom.", "Who owns it?", "Me"]
        );
    }

    #[test]
    fn the_test_meeting_has_tom_as_me() {
        let script = Script::test_meeting();
        assert_eq!(script.lines.len(), 12);
        assert_eq!(script.lines[0].source, Source::Me);
        assert_eq!(script.lines[1].source, Source::Them);
    }
}
