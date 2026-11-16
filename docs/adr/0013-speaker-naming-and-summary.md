# 0013: Speaker naming is checked in code; the summary keeps its data and ticks

## Context

Processing's steps 2 and 3 (#11) call the Summary and names job's AI. A wrong speaker name is worse than "Speaker 3" (DESIGN §4.3), and an LLM sometimes invents evidence or guesses from roles. The summary must render as Markdown (summary.md, export) and as the Summary tab, with next steps the user ticks. Meetings processed before #11 have the two steps recorded done with nothing written (ADR 0010).

## Decision

- **Naming** (`recap::naming`): Me's name is Settings' "my name", set without the AI. The voices `S1..Sn` go to the AI only when one is still unnamed. It gets the refined transcript with turns labelled by id and answers `{S1: {name|null, evidence, confidence}}`. The schema has one required entry per voice; every object is closed (#10). The answer is kept as `names.json`. `naming::accept` takes a name only when all of these hold:
  - confidence is `high`;
  - the evidence, compared as lowercase words, is a quote of the transcript;
  - the name is in its evidence;
  - it isn't a placeholder ("Speaker 2", "S1", "Me") or the user's own name, since the user is on the mic;
  - no other voice got it, and no speaker already has it.
  Names already given (a rename, a rerun) are never replaced.
- **Summary** (`recap::summary`): the AI gets the transcript labelled as the Transcript tab shows it, My notes and the meeting's date in words. It answers `SummaryData {title, overview, topics[{heading, bullets}], next_steps[{owner|null, text, due|null}], decisions[]}`. This is stored as `Summary { overview, markdown, data }`. The Markdown is the overview, then a `###` section per topic, then `### Next steps` as a task list (`- [ ] **Owner**: text (due)`), then `### Decisions`. The meeting takes the title unless the user changed it, that is, unless its title is neither empty nor the previous generated one.
- **Ticks** live in the data (`next_steps[i].done`, left out when false), and the Markdown is rendered again (`[x]`). `summary::set_done` writes both. A summary written again keeps the tick of any step whose text is unchanged.
- **Regenerate**: `processing::redo(folder, steps)` marks steps pending in `processing.json`, and the host's `regenerate_summary(id)` redoes Naming speakers + Writing summary and processes the meeting. This is how meetings processed before #11 get names and a summary. The Summary tab offers it ("Write summary") when a meeting has none.
- **My notes** are sent to the store at Stop (`set_my_notes`), so the summary step reads them from the meeting.
- **Due dates** are strings the AI writes ("Thu, Nov 19" or "Before Dec 1"). The meeting's date is given in UTC: there is no time-zone crate, and a day's error near midnight is accepted.

## Consequences

- Fewer names than the AI suggests: a voice the AI was only medium-sure about stays "Speaker N" until the user names it (#12).
- Regenerating costs one naming call more than strictly needed. Naming skips voices already named, so it never undoes a rename.
- `summary.json`'s `data` is the AI's answer plus `done` flags. Export (#13) can use `markdown` as is.
