# SSH agent forwarding

**Mode:** Formal

## Intent

Add SSH agent forwarding — the classic `ssh -A` behaviour. When enabled, the remote side can reach the local ssh-agent over a back-channel, so tools running on the remote (git, further ssh hops, and the like) use the local keys without those keys ever leaving the local machine. Applies to the destination hop; jumphosts already authenticate locally against our agent. Off by default; turned on by the invocation flag or by `ForwardAgent yes` in `~/.ssh/config`.

## Approach

### Request forwarding only on the destination session

`interactive.rs` and `command.rs` both open the session channel that carries the foreground work. That channel is where OpenSSH's `-A` sends `auth-agent-req@openssh.com`, and where sshd sets `SSH_AUTH_SOCK` for the remote environment. Jumphost sessions never see the request; the user's remote-side tools only run at the destination.

### Bridge incoming forward channels to the local agent

russh delivers `auth-agent@openssh.com` opens to `Handler::server_channel_open_agent_forward`. Implement it by spawning a task per channel that opens a fresh connection to the local agent — reusing the `connect_to_agent` platform split already in `authentication.rs` — and pumps bytes both ways with `tokio::io::copy_bidirectional` until either side closes. A dedicated connection per channel mirrors how ssh-agent handles concurrent clients and avoids a shared-lock bottleneck.

### Only the destination may open agent channels

Every hop shares one `Client` handler, so the bridge is gated: only the destination's handler, built with `Client::forwarding_agent`, bridges agent channels. On any other connection — jumphosts included — a server-opened agent channel is closed, since nothing requested it. Ungated, a hostile jumphost could use the local keys. Matches OpenSSH.

### One boolean threaded through `run`

A new `-A` flag on `ConnectArgs`, and a matching `forward_agent: Option<bool>` on `HostParams` for `ForwardAgent yes|no`. `main.rs` resolves the pair — CLI wins, else config, else false — and passes the resolved boolean into `run` alongside `reconnect` and `verbose`. Same shape as the existing options.

### `-A` is a no-op under `-N`

With `-N` no session channel is opened, so there is nowhere to send the request and nothing remote to use it — the same as OpenSSH, which also skips the request without a session.

### Refusal keeps the session

If the destination server rejects the request (`want_reply: true`), log at `-v` and continue rather than bail. Matches OpenSSH and avoids surprising a user whose earlier session worked without `-A`.

### Config parser extension

Add `ForwardAgent` to `ssh_config::parse`, storing `Option<bool>` so unset is distinguishable from explicit `no`, with a unit test for parse and default.

## Plan

- [x] Add `forward_agent: Option<bool>` to `HostParams` in `ssh_config.rs`; parse `ForwardAgent yes|no`; unit tests cover parse and absent default.
- [x] Refactor the local-agent connect in `authentication.rs` so the raw platform stream is reachable independently of the `AgentClient` wrapper — a shared helper both auth and the forward bridge can call.
- [x] Implement `Handler::server_channel_open_agent_forward` on `Client` in `connection.rs`: per incoming forward channel, spawn a task that opens the local agent stream and runs `tokio::io::copy_bidirectional` until either side closes. If the local agent is unreachable, log at `-v` and drop the channel; the session continues.
- [x] Add an `-A` flag to `ConnectArgs` in `main.rs`; resolve CLI-else-config-else-false and thread the resolved boolean through `execute` into `run`.
- [x] Extend the `run` entry point to accept `forward_agent: bool` and propagate it to the destination session's foreground path.
- [x] In `interactive.rs` and `command.rs`, call `channel.agent_forward(true)` on the destination session's channel when the flag is on; on a rejection reply, log at `-v` and continue.
- [x] Update `README.md` — add `-A` to the flag list and note `ForwardAgent yes` in `~/.ssh/config`.
- [x] `cargo build` and `cargo test`; smoke-test against a real remote: `sshtool -A alice@remote` then `ssh-add -L` on the remote lists the local keys; `ForwardAgent yes` in `~/.ssh/config` triggers it without `-A`.

## Log

- 2026-10-22 — Deviation (security): `Client` is the one handler for every hop, so an ungated bridge would let a jumphost or server open an unrequested agent channel and use the local keys. `Client` now carries a `forward_agent` flag, set only on the destination via `Client::forwarding_agent(verbose)`. Any other agent channel is closed, as OpenSSH does. `connection::connect` and `connect_with_passwords` therefore take the destination's handler.

- 2026-10-22 — `request_agent_forwarding` waits for the server's reply right after the session channel opens, before the PTY/exec requests, so the reply can't be confused with theirs.

- 2026-10-22 — `key_install.rs` had its own copy of the agent connect; it now uses the shared helper in `authentication.rs`.

- 2026-10-22 — README has no flag list; `-A` went into the usage line, the flags paragraph and an example instead.

- 2026-10-22 — Item 8 partial: `cargo build`, `cargo test` (22 pass), clippy (no new warnings) and a Windows-target `cargo check` are clean. Blocked on the smoke test: no sshd on this machine, so the real-remote check (`ssh-add -L` on the remote; `ForwardAgent yes` without `-A`) needs the user.

- 2026-10-22 — Version bumped 0.14.1 → 0.15.0 for hand-back, on the user's go-ahead; CHANGELOG entry left for Conclude.

- 2026-10-22 — Item 8 complete: user smoke-tested against a live host and reports it works.

## Conclusion

Completed. `-A` and `ForwardAgent yes` forward the local agent to the destination only; each server-opened agent channel gets its own local-agent connection. Beyond the first draft: the bridge is gated per connection so no jumphost or unrequested channel reaches the agent (added to the Approach), and `-A` is a no-op under `-N`. `key_install` now shares the agent connect. User smoke-tested on a live host. No map yet — see `10-bootstrap-project-map`. Ships 0.15.0.
