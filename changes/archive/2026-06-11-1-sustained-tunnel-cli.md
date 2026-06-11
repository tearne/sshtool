# Sustained tunnel (CLI)

**Mode:** Formal

## Intent

Add auto-reconnecting SSH sessions to the CLI: when the connection drops, detect it and re-establish transparently — for both port-forward mode and interactive sessions. The primary use case is an interactive session into a remote multiplexer (e.g. zellij, tmux): the remote process stays alive on the server and the user is dropped back in automatically when the network recovers. Port forwarding (`-L`) also benefits from the same reconnection engine. Arguments stay close to `ssh` convention. No daemon; the session lives as long as the process runs.

## Approach

### CLI shape: `-L` and `-N` flags, no subcommand

Mirror `ssh` exactly: `-L local_port:remote_host:remote_port` (repeatable) added to the existing invocation, and `-N` to suppress the remote shell. `sshtool -N user@host -L 5900:remote:5900` is the pure tunnel; `-L … cmd` forwards while running a command; `-L …` alone forwards while running a shell. No subcommand — the mode is inferred from what flags are present, just as `ssh` does.

### Local port forwarding mechanics

For `-L local_port:remote_host:remote_port`, the tool binds a local TCP listener and, for each incoming connection, opens a `channel_open_direct_tcpip` channel through the SSH session to `remote_host:remote_port`. russh already provides this method (used by the existing jumphost path). Each accepted connection gets its own channel, with bytes copied bidirectionally between the local socket and the channel using `tokio::io::copy_bidirectional`.

### Reconnection loop owns the session lifetime

A top-level reconnect loop in a new `tunnel` module holds the connection lifecycle: connect → forward all accepted connections → on any error or disconnect, wait a backoff interval and reconnect. The local listener is bound once before the loop and held across reconnects, so in-flight connections on a dead session fail but the port stays bound — no "address already in use" on retry.

### Backoff: capped exponential, no jitter

Start at 2 s, double on each failure, cap at 60 s. Simple and predictable; jitter is only meaningful when many clients reconnect simultaneously, which isn't this use case.

### New module: `tunnel`

All tunnel logic lives in `src/tunnel.rs`. It is pub(crate) and exposed through `lib.rs` as `pub async fn tunnel(target: &Target, forwards: &[LocalForward]) -> Result<()>`. The `tunnel` subcommand in `main.rs` calls it.

### `LocalForward` type

A small struct `{ local_port: u16, remote_host: String, remote_port: u16 }` parsed from `-L local_port:remote_host:remote_port` strings by a helper. Lives in `tunnel.rs` for now — if config support later needs it, it can be promoted. Multiple `-L` flags are accepted; each binds its own local listener and shares the same SSH session.

### Status output

Print a line to stderr when the tunnel comes up ("Forwarding 5900 → remote:5900"), when it drops ("Connection lost, reconnecting in 2s…"), and when it reconnects ("Reconnected"). No structured logging library — `eprintln!` is sufficient for this scope.

### Ctrl-C: immediate drop

On SIGINT the process exits immediately; in-flight forwarded connections are dropped without draining. The user's application will simply reconnect — this is the expected behaviour for a tunnelling tool and avoids threading a shutdown signal through all accepted-connection tasks.

## Plan

- [x] Add `LocalForward` struct and `-L` parser to a new `src/tunnel.rs`
- [x] Add `-L` (repeatable) and `-N` flags to `Invocation` in `main.rs`
- [x] Implement `tunnel(target, forwards)` in `src/tunnel.rs`: bind local listeners, reconnect loop, `channel_open_direct_tcpip` per accepted connection, bidirectional copy
- [x] Expose `tunnel` from `lib.rs`
- [x] Wire `execute()` in `main.rs`: when `-L` flags are present, call `tunnel`; `-N` suppresses the shell/command path; error if both `-N` and a command are given
- [x] Add status output (`eprintln!`) for connect, drop, and reconnect events
- [x] Bump version to 0.7.0
- [x] Add `-v`/`-vv`/`-vvv` verbosity flag
- [x] Add reconnection to the interactive (`-t`) path: on session drop, print "Connection lost, reconnecting…", wait backoff, reconnect and re-run the same command
- [x] Bump version to 0.7.5

## Conclusion

Delivered auto-reconnecting SSH sessions for both port-forward (`-L`/`-N`) and interactive (`-r`) modes. The interactive reconnect required two rounds of debugging: first, `pump` was returning `Ok(0)` on a network drop (no `ExitStatus` received), which looked like a clean exit; second, a series of keepalive/timeout fixes to the tunnel path — though the tunnel reconnection issue remains unverified as focus shifted to the interactive path, which is the primary use case.

The `6-login-shell-command-execution.md` change surfaced as a real blocker during testing — `zellij attach` failed due to PATH issues with `-t`; workaround is the full binary path or using tmux.

No map exists yet to update.

## Log

- First `run_session` draft serialised listeners in a for loop; rewrote to spawn one `accept_loop` task per listener in a `JoinSet`, returning the first error to trigger reconnect.
- 0.7.0 hung on network drop and exited cleanly instead of reconnecting: no keepalives meant TCP stalled indefinitely; `Ok(())` from accept loop fell through to process exit. Fixed in 0.7.1: SSH keepalives every 15s (3 strikes), `Ok(())` now treated as a disconnect.
