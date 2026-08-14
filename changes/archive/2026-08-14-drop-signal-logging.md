# Drop-signal logging

**Mode:** Formal

## Intent

Persist a timestamped record of every connection drop to a local log file, so the flakiness we're currently guessing at becomes something we can read back. For each drop, record which of the existing detection signals fired — heartbeat timeout, foreground read error, or forwarding accept-loop error — and the raw error text, with no interpretation. The point is to observe what real drops look like before deciding how much cause-attribution is worth building.

This is a deliberately lean precursor to the full diagnostics change (`5-diagnostics.md`), which layers best-effort local-vs-remote attribution and log export on top. This slice just gets the raw signals onto disk.

## Approach

### A `diagnostics` module owns the log

A new module holds the drop-signal enum, the log-line format, and the append-to-file writer. Keeping it separate from `session` means the supervisor just names the event; the module decides where and how it lands. The file is `~/.sshtool/drops.log` (Windows `%USERPROFILE%\.sshtool\drops.log`) — self-contained and easy to export. Timestamps are UTC RFC3339, unambiguous when a log crosses machines to a support team, which needs a time-formatting dependency (the `time` crate) since none is present today.

### Tag each drop signal at its source

Today the three signals collapse into one generic `Round::Dropped(error)` — the supervisor can't tell which fired because `first_failure` only sees an opaque `JoinSet` return. The fix carries a signal kind alongside the error: heartbeat and forwarding-accept each return their category, and a foreground read error is tagged where `serve` catches it. This attribution is the whole reason the slice exists, so it's load-bearing, not incidental.

### Log connects as well as drops

A drop timestamp is meaningless without knowing when the session started. Each successful connect and each connect-failure during reconnect is logged too, so the file reads as a session lifetime — established, ran N minutes, dropped via signal X.

### Always on, appended, capped to recent events

The log writes unconditionally, independent of `-v`, because the drops we're chasing are unexpected — a flag the user must remember defeats the purpose. Events are low-volume (connect/drop only), one line each. The file is capped at the most recent 1000 events: on write, if the cap is exceeded the oldest lines are dropped and the tail rewritten. At this volume rewriting is cheap, and it keeps the file bounded without rotation files to manage or export. The cap is a single named constant, easy to tune.

### Separate from the `log`/`env_logger` facade

The existing `-vvv` wiring feeds russh's noisy trace output to stderr. This file is a dedicated, always-on writer so it stays clean and independent of verbosity.

## Plan

- [x] Add the `time` dependency for RFC3339 UTC timestamps
- [x] Create a `diagnostics` module: drop-signal enum, log-line format, and a capped append writer to `~/.sshtool/drops.log` with cross-platform home resolution
- [x] Carry a drop-signal kind through the supervisor — heartbeat, forwarding accept-loop, and foreground errors each tagged, replacing the opaque `Round::Dropped` error
- [x] Log connect-established, connect-failure, and drop events from `supervise`

## Log

- Verified in-environment: `CONNECT_FAILED` lines are written with UTC RFC3339 timestamps, events append across runs, and the 1000-event cap trims the oldest lines. The `CONNECTED` and `DROPPED` paths couldn't be exercised here (no reachable SSH server, and the interactive/raw-mode path needs a TTY), but they share the same writer and are wired symmetrically at the connect and drop sites.

## Conclusion

Completed. A `diagnostics` module writes connect / connect-failure / drop events to `~/.sshtool/drops.log`, UTC RFC3339, capped at 1000 events. Drops are attributed to the signal that noticed — heartbeat, forwarding, or foreground — by tagging each at source and carrying it through `Round::Dropped`. Added the `time` dependency. `DROPPED` lines weren't exercised (needs real drops) but share the verified writer; the user confirmed `CONNECTED` and file creation on a live host. This is the lean precursor to `5-diagnostics.md`, which will add local-vs-remote attribution and export. Shipped 0.14.1.
