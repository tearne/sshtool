# Zellij shorthand flag

**Mode:** Formal

## Intent

Add a `-z` flag so `sshtool -z host` is shorthand for `sshtool -r host -- "zellij attach -c main"` — reconnect to the host and attach to the `main` Zellij session, creating it if it doesn't exist. This is the common "resume my multiplexer" workflow, and reconnect already exists to pair with it; `-z` makes it a single keystroke instead of a remembered incantation.

## Approach

### `-z` expands at the argument boundary

`-z` is a boolean flag that, in `execute`, sets reconnect and supplies the command `zellij attach -c main`. It's pure shorthand — no new runtime behaviour, no new `Foreground` variant. Because it turns reconnect on, the command already flows through the interactive login-shell path (the PTY behaviour Zellij needs), so nothing downstream changes.

### `-z` owns the command, so command-supplying flags conflict

`-z` provides the remote command itself. Combining it with a trailing command, or with `-N` (no shell), is contradictory and bails with a clear message — mirroring the existing `-N`-plus-command guard. An explicit `-r` alongside `-z` is redundant, not contradictory (both mean reconnect), so it's accepted silently — the usual CLI convention for idempotent flags.

## Plan

- [x] Add `-z` boolean flag to `ConnectArgs` with help text describing the Zellij attach-or-create shorthand
- [x] In `execute`, expand `-z` into reconnect plus the `zellij attach -c main` command, and bail when `-z` is paired with a trailing command or `-N`
- [x] Add a `-z` example to the README usage block

## Log

- During review the shorthand command was changed from `zellij a || zellij` to `zellij attach -c main`. The `|| zellij` fallback spawns an extra session whenever two or more already exist (proliferation) and, under `-r`, an ambiguous-attach error could cause a reconnect spin. Attaching to a fixed-named session (`main`) is deterministic: it always resumes the same session, creates it when absent, and never errors or proliferates.

## Conclusion

Completed. `-z` expands (`expand_zellij_shorthand`) into reconnect + `zellij attach -c main`, with conflict guards for a trailing command and `-N`. The command was chosen during review over `zellij a || zellij` to avoid session proliferation and reconnect spin (see Log). No map. Shipped 0.13.2.
