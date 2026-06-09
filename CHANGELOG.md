# Changelog

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
