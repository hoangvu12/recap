# 0010: Processing is a resumable job queue of steps

## Context

After Stop a meeting goes through Transcribing → Naming speakers → Writing summary (#9, #11). Each step calls a provider that can fail (quota, key, network) or be cut off (the app is killed, the PC crashes). The user must be able to Retry from the failed step and Recover old meetings without redoing (and paying for) what already worked.

## Decision

- `recap::processing` (app crate, no zgx) holds the queue. A `Step` (`id`, `run(Job) -> Result<(), String>`) keeps its output in the meeting's folder or the store before it returns Ok. The queue writes `processing.json` (`Progress`: each step pending/running/done/error, took_ms, error) after every change and tells the host (`ProcessingEvent::{Progress, StepDone, Finished, Stopped}`).
- Running it again skips steps that are `done` in `processing.json`; `running` (killed) and `error` start over. Steps resume inside themselves where it pays: Transcribing keeps each side's batch answer as `stt_them.json` / `stt_me.json` as soon as it comes and asks only for the missing side.
- The meeting is `Done` only when every step is; until then Home shows it as unfinished. Opening an unfinished meeting processes it again (Recover); `retry_processing(id)` does the same from the card.
- Transcribing: both sides in parallel (Them `diarize=true`, Me not; settings' keyterms; a silent side is skipped). `recap_engine::refine::turns` splits words into sentences (speaker change, `.?!`, or 1.5 s of silence), drops Me sentences that are echoes by the live rule (`echo::is_echo`), sorts by time and joins a speaker's sentences in a row into turns. Speakers: `me` first, then `S1..Sn`; names given before stay.
- Naming speakers and Writing summary are `Later` slots (done at once) until #11 puts its steps in with `Processing::with(step)`.
- The UI gets `processing = { id, steps }` for the meeting the Notes window shows (no steps when there is nothing to show), and `transcript.generation`, bumped whenever the presenter is replaced, so the refined rows replace the live ones even at the same row count.

## Consequences

- A step that fails leaves earlier output in place; Retry costs only the failed part.
- Meetings processed before #11 lands are `Done` with no names or summary; #11 needs its own way to fill those (Regenerate) since done steps don't run again.
- `stt_*.json` and `processing.json` are not indexed: the folder stays the truth, the DB is rebuilt from it as before.
