# 0019 Me is always transcribed live; "Caption my own speech" is overlay-only

Supersedes the parts of ADR 0009 ("With Caption my own speech off, nothing is judged") and ADR 0012 (`MeetingControl::caption_me`) that tied the Me socket to the setting.

## Context

Stories 21 and 29 want my own lines in the Notes live transcript, and echo dedup needs Me's finals to judge. Story 15 only asks that "Caption my own speech" decide whether my lines show in the overlay. The first build opened the Me live socket only while the setting was on, so with the default (off) Notes had no Me rows and nothing of Me was saved live.

## Decision

- The engine always runs both live sockets (Them and Me). `MeetingConfig.caption_me` and `MeetingControl::caption_me` are gone.
- Echo dedup always runs on Me finals (ADR 0009's rule, unchanged).
- "Caption my own speech" is the presenter's `my_captions`: it filters the overlay's `captions` only. Toggling it mid-meeting (`Host::set_my_captions`) changes no socket.

## Consequences

- A meeting always costs two live sessions with the provider.
- `live.ndjson` holds Me finals whatever the setting.
- A Me provider failure is a notice (ADR 0012 unchanged): Them drives the overlay's status.
