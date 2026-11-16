# 1. Final and partial text in one paragraph: `ui.rich` spans

Status: accepted (ticket #2)

## Context

A live row and a caption line show final words in one colour and the
partial after them in another (motion_spec §2, screens 1 and 2), wrapping as
one paragraph. Two text elements side by side can't wrap together, so zgx has
to draw two colour runs in one text block, or we need a fallback.

## Decision

zgx can draw them. `ui.rich { ui.span { final, color = … }, ui.span { partial, color = … } }`
is one paragraph with a colour per run (zgx docs/reference/text.md), and each
span's text and colour can be a function that follows state. The transcript row
(`ui/components/transcript_row.luau`) and the overlay line (`ui/screens/overlay.luau`)
use it. Checked in the running app: the final words are primary and the partial is
tertiary, wrapping together.

No fallback is needed. `win:text("#id")` in `zgx test` reads the whole
paragraph, final and partial joined.

## Consequences

- The partial span puts the space before itself (`" " .. partial`) when there
  is final text, so the paragraph reads as one sentence.
- Motion on the partial (fade-in per word, motion_spec) has to work on spans,
  not elements: ticket #8 decides how.
