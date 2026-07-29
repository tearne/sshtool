# Changelog

## 0.9.5

- `--keygen`, `--copy-id`, and `--install` (Windows) are now first-class flags listed in `--help`, replacing the previous hidden subcommand-style routing. The default `sshtool user@host` invocation is unchanged. Flags were chosen over subcommands to avoid collisions between command names and hostnames / ssh-config aliases.

## 0.9.4

- Read connection settings from `~/.ssh/config`: `HostName`, `User`, `Port`, and `ProxyJump` (single hop). Explicit CLI flags take precedence. `sshtool myalias` now honours whatever the user has set up for `ssh`.

## 0.8.10

- `sshtool install` (Windows): copies the binary to `%USERPROFILE%\bin` and adds it to the user PATH via the registry. No admin rights required. Idempotent — safe to run again from the installed location.

## 0.8.8

- `sshtool keygen`: generate an Ed25519 keypair and load it into the SSH agent. Discovers existing keys by scanning `~/.ssh/` for OpenSSH private key files rather than fixed names. Enforces exactly one key in agent. Passphrase support with prompts matching `ssh-keygen`.
- `sshtool copy-id user@host [-J jumphost]`: install the agent's key on every hop, using `printf | tee -a` for rbash compatibility. Shares a password store across hops to avoid double prompting. Verifies key auth via a fresh connection after each install.

## 0.8.4

- `sshtool keygen`: generate an Ed25519 keypair and load it into the SSH agent; discovers existing keys by scanning `~/.ssh/` for OpenSSH private key files rather than fixed names. Passphrase support matching `ssh-keygen` conventions. Enforces exactly one key in agent.
- `sshtool copy-id user@host [-J jumphost]`: install the agent's key on every hop, verifying auth after each install. Uses `printf | tee -a` for compatibility with restricted shells (rbash).
- `-r`/`--reconnect`: auto-reconnecting interactive sessions — on drop, reconnects and re-runs the same command (e.g. `sshtool -r -t user@host tmux attach`).
- `-L`/`-N`: local port forwarding with auto-reconnection, matching `ssh` flag conventions.
- `-v`/`-vv`/`-vvv`: verbosity flag for connection diagnostics.
- Strip debug symbols from release builds (7.5MB, down from 9.8MB).

## 0.6.0

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
