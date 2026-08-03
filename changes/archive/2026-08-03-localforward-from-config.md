# LocalForward from ssh config

**Mode:** Formal

## Intent

When `~/.ssh/config` defines `LocalForward` entries for a host, `sshtool` should honour them automatically on connection — the same way it already honours `HostName`, `User`, and `ProxyJump`. Forwards from the config are merged with any `-L` flags given on the command line. The combined set is active for all connection paths (interactive, command, and explicit tunnel).

## Approach

The guiding principle is consistency with `ssh`: forwards run *alongside* whatever session you'd otherwise get, and `-N` is the modifier that suppresses the shell. This reshapes three concerns — config parsing, the trigger model, and the session/reconnect structure.

### Parse `LocalForward` into an accumulating field

`HostParams` gains `local_forwards: Vec<String>`. Unlike the single-value directives, every matching `LocalForward` line accumulates (ssh applies all of them). Config and `-L` feed one shared parser, extended to carry an optional bind address: `LocalForward`'s two-token `[bind:]port host:hostport` form and `-L`'s `[bind:]port:host:hostport` colon form both resolve to a forward with a bind address (default `127.0.0.1`), local port, remote host, and remote port. The `LocalForward` struct grows a `bind_address` field to hold it.

### Forwards merge, and no longer force forward-only

Config forwards and `-L` flags concatenate into one forward set. The mere presence of a forward no longer routes to a forward-only path; instead forwards run concurrently with the foreground activity (shell or command). `-N` becomes the switch that selects forward-only, matching `ssh -N`.

### One session supervisor replaces the three connect loops

The forward-only `tunnel` loop, `reconnecting_interactive`, and the plain interactive/command calls collapse into a single supervisor. It binds the forward listeners once up front (kept across reconnects, as `tunnel` already does), then per connection spawns the forward accept loops, the heartbeat, and the foreground activity concurrently on one shared `Session`. The foreground task decides completion: a clean shell/command exit ends the supervisor and tears down the forwards with it, matching `ssh`. A session drop triggers reconnect only under `-r`; without `-r` a drop ends the session, as `ssh` does — so forward-only `-N` sessions no longer reconnect forever the way the old `tunnel` path did. `-N` runs the same supervisor with no foreground task.

### Bind the requested address, warn and continue on failure

Each forward binds the address it names, defaulting to `127.0.0.1` as `ssh` does. Binding is per-forward and non-fatal: a failure (privileged low port, address in use, address not local) prints a warning naming the forward and the reason, then that one forward is skipped while the rest and the foreground session carry on. This mirrors `ssh`'s default `ExitOnForwardFailure no` — a forward that can't bind never takes the whole session down with it.

## Plan

- [x] Add `local_forwards: Vec<String>` to `HostParams` and accumulate every matching `LocalForward` line (two-token `[bind:]port host:hostport` form) in the config parser.
- [x] Give `LocalForward` a `bind_address` field and extend `parse_forward` to accept an optional bind prefix, covering both the config two-token form and the `-L` `[bind:]port:host:hostport` colon form.
- [x] Merge config `local_forwards` with `-L` flags into one forward set at target resolution.
- [x] Switch the forward-only trigger from "any `-L`" to `-N`, so forwards otherwise run alongside the foreground activity.
- [x] Replace the `tunnel`, `reconnecting_interactive`, and plain interactive/command entry points with one supervisor: bind listeners once, then per connection run forward accept loops, heartbeat, and the foreground activity on a shared session; foreground completion ends it, a drop reconnects only under `-r`.
- [x] Make listener binding per-forward and non-fatal: warn naming the forward and reason, skip it, continue with the rest.
- [x] Tests: `LocalForward` config parsing (accumulation, two-token form) and bind-address parsing for both forward syntaxes.

**Done when** config `LocalForward` entries and `-L` flags both take effect together, run concurrently with a shell or command, tear down on foreground exit, reconnect only under `-r`, and survive a failed bind with a warning; Linux build and Windows build pass.

## Log

- Build resumed and assessed after interruption; state reconstructed from the code since no ticks or Log existed. Plumbing tasks are complete: config parsing (task 1), `bind_address` + `parse_forward` (task 2), non-fatal per-forward binding (task 6), and tests (task 7), all with passing unit tests in `ssh_config.rs` and `tunnel.rs`.
- The `session.rs` supervisor (task 5) is fully written — `Foreground`, `run`, `supervise`, `serve`, `Terminal` — but **not wired in**: no `mod session` in `lib.rs`, not called from `main.rs`, and the old `tunnel`/`reconnecting_interactive` paths still stand.
- Build is currently broken: `interactive.rs` was mid-refactor extracting `run_once`; the old `request_pty` helper lost its `async fn request_pty(...)` header, leaving a dangling body (`unexpected closing delimiter`).
- Remaining work is the wiring layer — repair `interactive.rs`, declare/export `session`, replace `main.rs` dispatch with `session::run` + a `Foreground` from `-N`/command/shell (tasks 3, 4, finish 5), then remove the old entry points.
- Wiring completed: repaired the `interactive.rs` `request_pty` header; declared and exported the `session` module; replaced `main.rs` dispatch with `session::run`, deriving `Foreground` from `-N`/command/shell and merging config `local_forwards` with `-L` flags (config first). Removed the now-dead `run`/`interactive`/`reconnecting_interactive` lib entry points and the `connect` helper.
- Narrowed `tunnel::{bind_forwards, accept_loop, heartbeat, BoundForward}` to `pub(crate)` and dropped the unused `Session::active_mut`, clearing all warnings.
- Linux build, Windows (`x86_64-pc-windows-gnu`) check, and all 18 tests pass. Version bumped 0.11.0 → 0.11.1 for hand-back.

## Conclusion

Completed as planned. The build was interrupted mid-refactor and resumed: the plumbing (config parsing, `bind_address`/`parse_forward`, non-fatal binding, tests) was already in place; this session finished the wiring — repaired `interactive.rs`, wired the `session` supervisor into `lib.rs`/`main.rs`, merged config + `-L` forwards, and switched the forward-only trigger to `-N`. The old `tunnel`/`reconnecting_interactive`/one-shot entry points were removed. No `map.md` in this project, so no map impact. Ships as 0.11.1.
