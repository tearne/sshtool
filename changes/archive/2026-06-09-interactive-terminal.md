# Interactive terminal

**Mode:** Explore

## Intent

Today `sshtool` only runs a command and streams its output. The next step toward an `ssh`-shaped tool is an interactive terminal: run `sshtool user@host` with no command and land in a live remote shell — keystrokes, control keys, editors, and full-screen programs all working — exactly like `ssh user@host`.

This needs a pseudo-terminal on the remote with the local terminal wired to it in raw mode: local input to the remote, remote output to the local screen. It opens the door to the case that motivated all this — dropping straight into a `tmux`/`zellij` session on connect — and is the foundation the tunnelling and reconnect features will build on.

## Approach

### Interactivity emerges from the absence of a command, or `-t`

No positional command → request a remote **shell** over a PTY (the interactive console). `-t <command>` → run that command over a PTY (the `tmux attach` case). A command without `-t` keeps today's non-interactive streaming `run`. So `command` becomes optional. `-T` (force no PTY) is not needed yet — a plain command already runs without one.

### Cross-platform terminal via `crossterm` + tokio stdio

Use `crossterm` (pure Rust, Windows-capable) only for raw-mode toggling and reading the terminal size, and `tokio::io::stdin()`/`stdout()` for the byte pump — all cross-platform, so the Windows build keeps compiling. We deliberately do **not** use crossterm's parsed key-event stream: a passthrough must forward raw bytes, not re-encoded key events. (The russh example's `termion` + `tokio_fd` are Unix-only — rejected.)

### A raw-mode guard restores the terminal

Enter raw mode before the session and restore it on scope exit via a drop guard, so a panic or error still leaves the terminal usable.

### Resize: Linux `SIGWINCH` now, Windows later

On Linux a `SIGWINCH` handler reads the new size and sends `window_change`, so full-screen apps reflow. Windows has no `SIGWINCH`; its resize handling is deferred with the rest of Windows runtime work. Needs tokio's `signal` feature.

### Platform scope: Linux verified, Windows compiles

The shell, the `-t` command, and resize are tested on Linux. The Windows binary must keep cross-compiling (verified by the cross-build), but its runtime behaviour — raw console mode, resize — is tested later.

### Deferred

`~.`-style escape sequences stay parked — exit is by ending the remote shell/program (Ctrl-D, `exit`, detaching tmux) or the connection closing. Local-stdin forwarding for non-interactive commands also stays parked.

## Plan

**Topics**

- **PTY passthrough** — open a channel, request a PTY sized to the local terminal, start a remote shell or (with `-t`) the given command, and pump bytes both ways until the session closes, returning the remote exit status.

- **Raw terminal mode** — enter raw mode for the session and restore it on exit, even on error or panic.

- **CLI routing** — `command` becomes optional and `-t` is added: no command → shell, `-t <command>` → interactive command, a plain command → today's non-interactive `run`.

- **Window resize (Linux)** — track `SIGWINCH`, read the new size, and send `window_change` so full-screen apps reflow.

- **Library entry point** — expose the interactive session alongside `run`.

- **Windows keeps compiling** — confirmed by the cross-build; runtime behaviour deferred.

**Done when** — on Linux, `sshtool user@host` opens a usable interactive shell (editors and `tmux` render correctly and reflow on resize), `sshtool -t user@host tmux attach` drops into a tmux session, the local terminal is restored after exit, the non-interactive `run` path is unchanged, and `cargo build --target x86_64-pc-windows-gnu` still succeeds.

## Log

- Library API refactored: `Target` no longer carries the command (it's just the destination now). `run(target, command)` is non-interactive; new `interactive(target, command: Option<&str>)` opens a shell (`None`) or a PTY command (`Some`). The CLI routes by command presence and `-t`. Breaking API change, fine pre-1.0.
- New `interactive` module: `request_pty` sized to the local terminal, then `request_shell`/`exec`, then a `tokio::select!` pump (stdin→channel, channel→stdout, resize→`window_change`). Raw mode via a `crossterm` drop guard; resize via a cfg-gated `ResizeWatcher` (Unix `SIGWINCH`; a never-firing no-op elsewhere). Cross-platform stdio via `tokio::io::stdin/stdout`, avoiding the example's Unix-only `termion`/`tokio_fd`.
- Verified on Linux: build + clippy clean, 6 tests pass, CLI routing correct (no-command and `-t` both reach the interactive path; plain command still streams). Windows cross-build succeeds including the `#[cfg(not(unix))]` resize branch (`sshtool.exe`, ~10.2 MB).
- Not verifiable here: the live interactive experience (raw mode, tmux, resize reflow) needs a real SSH server and a TTY — left for the user. Windows interactive runtime is parked (`windows-interactive-support`).
- User testing surfaced a hang: after `exit`, the session sat on "logout" until a key was pressed. Cause is documented `tokio::io::stdin` behaviour — its blocking read can't be cancelled, so it stalls runtime shutdown once the session ends. Fixed by reading stdin on a dedicated OS thread that forwards chunks over an mpsc channel (cancel-safe `recv` in the pump); the detached thread is killed at process exit instead of blocking it. Needs tokio's `sync` feature. Bumped to 0.5.1.
- Real-host testing also showed `-t <command>` runs in a non-login shell, so profile-built `PATH` entries (e.g. `~/.cargo/bin`) are absent — `sshtool -t host zellij a` fails to find `zellij` though it works in the interactive shell. Faithful to `ssh`; parked as `login-shell-command-execution` for an opt-in fix.

## Conclusion

Done, and validated on real hardware: `sshtool host` opens a working interactive shell, `-t <command>` runs a PTY command (shell, `zellij`, clean `exit` all confirmed by the user), the non-interactive `run` path is unchanged, and the Windows binary still cross-compiles. Shipped at 0.5.1 after fixing an stdin-driven shutdown hang (tokio's uncancellable blocking stdin, replaced with a threaded reader).

Two findings parked rather than fixed here: `-t` commands run in a non-login shell so profile `PATH` additions are missing (`login-shell-command-execution`), and Windows interactive runtime is still untested (`windows-interactive-support`).
