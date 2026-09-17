# SSH agent forwarding

**Mode:** Formal

## Intent

Add SSH agent forwarding — the classic `ssh -A` behaviour. When enabled, the remote side can reach the local ssh-agent over a back-channel, so tools running on the remote (git, further ssh hops, and the like) use the local keys without those keys ever leaving the local machine. Applies to the destination hop; jumphosts already authenticate locally against our agent. Off by default; turned on by the invocation flag or by `ForwardAgent yes` in `~/.ssh/config`.

## Approach

### Request forwarding only on the destination session

`interactive.rs` and `command.rs` both open the session channel that carries the foreground work. That channel is where OpenSSH's `-A` sends `auth-agent-req@openssh.com`, and where sshd sets `SSH_AUTH_SOCK` for the remote environment. Jumphost sessions never see the request; the user's remote-side tools only run at the destination.

### Bridge incoming forward channels to the local agent

russh delivers `auth-agent@openssh.com` opens to `Handler::server_channel_open_agent_forward`. Implement it by spawning a task per channel that opens a fresh connection to the local agent — reusing the `connect_to_agent` platform split already in `authentication.rs` — and pumps bytes both ways with `tokio::io::copy_bidirectional` until either side closes. A dedicated connection per channel mirrors how ssh-agent handles concurrent clients and avoids a shared-lock bottleneck.

### One boolean threaded through `run`

A new `-A` flag on `ConnectArgs`, and a matching `forward_agent: Option<bool>` on `HostParams` for `ForwardAgent yes|no`. `main.rs` resolves the pair — CLI wins, else config, else false — and passes the resolved boolean into `run` alongside `reconnect` and `verbose`. Same shape as the existing options.

### `-A` is sent unconditionally, including with `-N`

Even with `-N` the forwarding request goes out, matching OpenSSH. A user who sets `-A` gets it, whether or not a shell will consume it, so behaviour does not drift from `ssh`'s.

### Refusal keeps the session

If the destination server rejects the request (`want_reply: true`), log at `-v` and continue rather than bail. Matches OpenSSH and avoids surprising a user whose earlier session worked without `-A`.

### Config parser extension

Add `ForwardAgent` to `ssh_config::parse`, storing `Option<bool>` so unset is distinguishable from explicit `no`, with a unit test for parse and default.

## Plan

- [ ] Add `forward_agent: Option<bool>` to `HostParams` in `ssh_config.rs`; parse `ForwardAgent yes|no`; unit tests cover parse and absent default.
- [ ] Refactor the local-agent connect in `authentication.rs` so the raw platform stream is reachable independently of the `AgentClient` wrapper — a shared helper both auth and the forward bridge can call.
- [ ] Implement `Handler::server_channel_open_agent_forward` on `Client` in `connection.rs`: per incoming forward channel, spawn a task that opens the local agent stream and runs `tokio::io::copy_bidirectional` until either side closes.
- [ ] Add an `-A` flag to `ConnectArgs` in `main.rs`; resolve CLI-else-config-else-false and thread the resolved boolean through `execute` into `run`.
- [ ] Extend the `run` entry point to accept `forward_agent: bool` and propagate it to the destination session's foreground path.
- [ ] In `interactive.rs` and `command.rs`, call `channel.agent_forward(true)` on the destination session's channel when the flag is on; on a rejection reply, log at `-v` and continue.
- [ ] Update `README.md` — add `-A` to the flag list and note `ForwardAgent yes` in `~/.ssh/config`.
- [ ] `cargo build` and `cargo test`; smoke-test against a real remote: `sshtool -A alice@remote` then `ssh-add -L` on the remote lists the local keys; `ForwardAgent yes` in `~/.ssh/config` triggers it without `-A`.
