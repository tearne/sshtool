# Version flag and a single password prompt

**Mode:** Formal

## Intent

Two quality-of-life improvements to the CLI.

First, `sshcmd` should report its own version. A standard `-V`/`--version` flag that prints the build version lets a user tell at a glance which build they are running — useful now that there are multiple.

Second, when both the jumphost and the target authenticate by password, the user is currently prompted once per hop. Where the same credentials apply to both, that is a prompt too many. This change considers whether a password entered once can serve both hops — and under what conditions reuse is appropriate (e.g. the same username on both) versus when a separate prompt is still warranted.

## Approach

### `-V`/`--version` via clap's built-in flag

Add `version` to the binary's `#[command(...)]`. clap then provides `-V`/`--version` automatically, sourcing the version from `CARGO_PKG_VERSION` at build time. No conflict with the existing `-J`. Pure wiring; no new logic.

### Reuse a password across hops, keyed by username

Prompt once per username within a single run and reuse it for later hops that authenticate as the same username. A hop whose username differs prompts separately — passwords almost always differ with the user, so matching-username is the condition under which reuse is safe and expected. This is exactly the common case the user hits: same account on jumphost and target, one prompt instead of two.

### Password store lives for one connection

A small username→password store is created in `connect()` and threaded to each hop's authentication. It exists only for that one `capture` call — passwords are never persisted. The agent path is unchanged and still tried first per hop; the store only backs the password fallback.

### Re-prompt once on reuse failure

If a reused password fails on a later hop (same username, unexpectedly different password), prompt once more for that hop and replace the stored value, rather than failing outright. Keeps the happy path to a single prompt while not locking the user out of the edge case.

### No public API change

The prompt and the store are internal to authentication; `Target` and the library surface are untouched. This is additive (new flag, fewer prompts), so it is a backward-compatible minor release.

## Plan

- [x] Enable clap's built-in `-V`/`--version` flag
- [x] Add a per-run password store, keyed by username, created in `connect()` and threaded to each hop's authentication
- [x] Back the password fallback with the store: reuse a matching username's password, re-prompting once if it fails

## Log

- `-V`/`--version` verified (`sshcmd 0.3.0`); build, clippy, and 6 tests green; Windows binary cross-built clean.
- The password-reuse behaviour itself can't be exercised here — it needs two hops both falling back to password auth against a real server. It is compile- and logic-verified only; the single-prompt experience is for the user to confirm on real hosts.

## Conclusion

Completed as planned, no deviations. `-V`/`--version` reports the build version, and password authentication now prompts once per username and reuses it across hops, re-prompting only if a reused password fails. All internal to authentication — no API change — hence a backward-compatible 0.3.0 minor release. The single-prompt behaviour is compile- and logic-verified here; exercising it end to end needs two password hops against a live server.
