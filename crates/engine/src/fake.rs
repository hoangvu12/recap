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
    looping: bool,
}

impl ScriptedLiveStt {
    pub fn new(script: Script) -> Self {
        Self { script, pace: Pace::default(), looping: false }
    }

    pub fn with_pace(mut self, pace: Pace) -> Self {
        self.pace = pace;
        self
    }

    /// Say the script again and again, for as long as audio comes (the soak
    /// run's endless meeting). Each round starts a line gap after the last.
    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }

    /// How long one round of the script takes, up to where the next starts.
    fn round_ms(&self) -> u64 {
        self.line_times().last().map_or(0, |(_, end)| end + self.pace.line_gap_ms)
    }

    /// When each script line is said, `(start_ms, end_ms)` of meeting time,
    /// as the replay says it: its first word to its last sentence's final.
    pub fn line_times(&self) -> Vec<(u64, u64)> {
        let p = self.pace;
        let mut out = Vec::new();
        let mut t = 0;
        for line in &self.script.lines {
            let start = t;
            let mut end = t;
            for sentence in sentences(&line.text) {
                end = t + sentence.split(' ').count() as u64 * p.word_ms + p.final_lag_ms;
                t = end + p.sentence_gap_ms;
            }
            out.push((start, end));
            t += p.line_gap_ms - p.sentence_gap_ms;
        }
        out
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
        // Looping: each round is the schedule shifted by a round's length.
        let round_ms = if self.looping && !schedule.is_empty() { self.round_ms().max(1) } else { 0 };
        Box::pin(async move {
            let mut heard_ms = 0;
            let mut next = 0;
            let mut base = 0;
            let mut partial: Option<LiveEvent> = None;
            while let Some(frame) = audio.recv().await {
                heard_ms += frame.duration_ms();
                loop {
                    if next == schedule.len() && round_ms > 0 {
                        next = 0;
                        base += round_ms;
                    }
                    let Some(event) = schedule.get(next).filter(|e| base + e.end_ms <= heard_ms) else { break };
                    let event = LiveEvent { start_ms: base + event.start_ms, end_ms: base + event.end_ms, ..event.clone() };
                    partial = (!event.is_final).then(|| event.clone());
                    if events.send(event).is_err() {
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
    fn line_times_follow_the_replay() {
        let stt = ScriptedLiveStt::new(Script::test_meeting());
        let times = stt.line_times();
        assert_eq!(times.len(), 12);
        assert_eq!(times[0].0, 0);
        assert!(times.windows(2).all(|w| w[0].1 < w[1].0), "lines follow each other");
        let last_final = stt.schedule().iter().filter(|(_, _, e)| e.is_final).map(|(at, _, _)| *at).max();
        assert_eq!(Some(times[11].1), last_final);
    }

    /// The finals `stt` says on `source` over `heard_ms` of silence.
    fn finals(stt: &ScriptedLiveStt, source: Source, heard_ms: u64) -> Vec<LiveEvent> {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let (audio_tx, audio) = mpsc::unbounded_channel();
            let (events, mut out) = mpsc::unbounded_channel();
            for at in (0..heard_ms).step_by(100) {
                audio_tx.send(AudioFrame::silence(source, at, 100)).unwrap();
            }
            drop(audio_tx);
            stt.run(source, LiveOptions::default(), audio, events).await.unwrap();
            let mut finals = Vec::new();
            while let Ok(event) = out.try_recv() {
                if event.is_final {
                    finals.push(event);
                }
            }
            finals
        })
    }

    #[test]
    fn a_looping_script_says_it_again_later() {
        let once = ScriptedLiveStt::new(Script::test_meeting());
        let round = once.round_ms();
        let first = finals(&once, Source::Them, round * 3);
        let looping = ScriptedLiveStt::new(Script::test_meeting()).looping();
        let thrice = finals(&looping, Source::Them, round * 3);
        assert!(!first.is_empty());
        assert_eq!(thrice.len(), first.len() * 3, "three rounds in three rounds' time");
        let second = &thrice[first.len()..first.len() * 2];
        for (a, b) in first.iter().zip(second) {
            assert_eq!(a.text, b.text);
            assert_eq!(b.start_ms, a.start_ms + round, "the next round comes a round later");
        }
        assert!(thrice.windows(2).all(|w| w[0].start_ms <= w[1].start_ms), "time only goes forward");
    }

    #[test]
    fn the_test_meeting_has_tom_as_me() {
        let script = Script::test_meeting();
        assert_eq!(script.lines.len(), 12);
        assert_eq!(script.lines[0].source, Source::Me);
        assert_eq!(script.lines[1].source, Source::Them);
    }
}
