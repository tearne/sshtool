# Changelog

## 0.7.6

- Fix `-r` reconnection: channel closing without an ExitStatus (network drop) now correctly triggers reconnection instead of exiting cleanly with status 0.

## 0.7.5

- Add `-r`/`--reconnect` flag: auto-reconnecting interactive sessions. On network drop, waits backoff (2s–60s) and reconnects, re-running the same command. Designed for use with remote multiplexers — `sshtool -r -t user@host zellij attach` drops back into the existing zellij session when the network recovers.

## 0.7.4

- Add `-v`/`-vv`/`-vvv` verbosity flag: `-v` prints connection lifecycle events, `-vv` adds keepalive send/receive detail, `-vvv` enables russh internal trace logging to stderr.

## 0.7.3

- Fix reconnect hang: wrap each connection attempt with a 30s timeout so that when the network is still down during a reconnect, the TCP connect doesn't block indefinitely — it times out and the backoff loop retries.

## 0.7.2

- Fix reconnection detection: add an explicit heartbeat task inside each session that pings the server every 15s with a 15s reply timeout. Previously the russh keepalive ran in its own internal task but the accept loops had no way to observe session failure, so a dead network still hung. Now the heartbeat fails into the same JoinSet as the accept loops, triggering reconnection within ~30s of a network drop.

## 0.7.1

- Fix tunnel reconnection: SSH keepalives (every 15s, 3 strikes) now detect a dead connection and surface an error promptly instead of hanging indefinitely when the network drops.
- Fix silent tunnel exit: an accept loop returning without error now triggers reconnection rather than exiting the process cleanly.

## 0.7.0

- Auto-reconnecting local port forwarding: `sshtool -N user@host -L local_port:remote_host:remote_port` binds a local TCP listener and tunnels connections through SSH, surviving transient network loss by reconnecting automatically (exponential backoff, 2s–60s). Multiple `-L` flags are accepted. `-N` suppresses the remote shell for forward-only sessions; omitting it runs a shell or command alongside the forwards, matching `ssh` behaviour.

## 0.6.0

- Windows interactive terminal support: raw console mode with `ENABLE_VIRTUAL_TERMINAL_INPUT`/`ENABLE_VIRTUAL_TERMINAL_PROCESSING` so keystrokes (arrows, function keys) pass through as a VT byte stream and remote ANSI renders; window resize handled by polling the console size (no `SIGWINCH` on Windows). Console modes restored on exit/error/panic via the raw-mode guard.

Note: the Windows interactive path is verified by cross-compilation and type-checking only; live runtime behaviour (raw input, ANSI rendering, resize reflow) has not yet been tested on a real Windows terminal.

## 0.5.1

- Interactive terminal: with no command, `sshtool user@host` opens a remote shell over a PTY; `-t <command>` runs a command interactively (full-screen programs like `tmux`/`zellij`, `top`, editors). Raw-mode passthrough, with window resize forwarded on Linux.
- Library (breaking): `Target` no longer holds the command. Use `run(&target, command)` to run-and-stream, or `interactive(&target, command)` for a shell (`None`) or a PTY command (`Some`).

Note: interactive sessions are verified on Linux; Windows runtime behaviour (raw console, resize) is not yet tested.

## 0.4.1

- Renamed from `sshcmd` to `sshtool`.
- Command output now streams to the terminal (stdout and stderr) and the process exits with the remote command's status; clipboard capture removed.
- The leading `--` before the command is now optional (SSH-style): `sshtool host cmd…`.
- Library: `capture(&Target) -> Result<String>` is now `run(&Target) -> Result<u32>`.

Migration: the binary and crate are now `sshtool`; to put output on the clipboard, pipe it (`… | clip` on Windows). Library callers use `run` (returns the remote exit status) in place of `capture`.

## 0.3.0

- `-V`/`--version` prints the build version.
- Password authentication prompts once per username and reuses it across hops, re-prompting only if a reused password fails.

## 0.2.0

- `-J [user@]host` is the new jumphost flag (SSH `ProxyJump` alignment); `--jumphost` kept as a long alias.
- The jumphost authenticates with its own username when given, otherwise the destination's.
- Connection failures now name the host and hint it may be unknown or unreachable.
- Library (breaking): `parse_destination`/`Destination` are now `parse_endpoint`/`Endpoint`, and `Target.jumphost` is `Option<Endpoint>`.

## 0.1.0

- New `sshcmd` library: run a command on a remote host (optionally via a jumphost) over SSH and capture its stdout, authenticating with ssh-agent then password.
- New CLI: `sshcmd user@host [--jumphost <host>] -- <command...>` — prints the captured output and copies it to the clipboard.
- README documents native and Linux→Windows cross-compilation (`ring` crypto backend; `mingw-w64`). The cross-built `.exe` is standalone — no installation or admin rights to run.
