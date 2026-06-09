# Kickoff: run a remote command through a jumphost

**Mode:** Formal

## Intent

A user on a locked-down Windows laptop needs the contents of a text file that lives on a remote host they cannot reach directly. They can only reach it by SSH: hop to a jumphost, hop from there to the remote host, run a command, and read what it prints. Today that means doing every hop by hand and copying text out of a terminal.

`sshcmd` is a minimal demonstrator that collapses those hops into one action. Given a remote host address, a username, a command, and optionally a jumphost to route through, it runs the command on the remote host and captures whatever the command prints to standard output as a `String`.

The deliverable is a Rust **library** that performs this capture, plus a thin **Windows command-line binary** that demonstrates it. The user invokes the binary with the destination host, the command, and — if applicable — a jumphost; the command's captured output is both printed to the terminal and copied to the clipboard, ready to paste.

Authentication follows SSH's own behaviour: if an ssh-agent is available and holds a usable key, use it; otherwise fall back to prompting for a password. The single configured username applies to both hops.

This first change aims for something that *works end to end* for the happy path — not for completeness. Robust error handling, configuration richness, and per-hop credential variety are explicitly out of scope for now.

## Approach

### SSH mechanism: `russh` (pure-Rust async client)

We use the `russh` crate rather than `ssh2` (libssh2 bindings) or shelling out to the system `ssh`. Two reasons. First, pure Rust means no C dependencies (libssh2/OpenSSL), so the Windows binary builds with only the Rust toolchain. Second, and decisively for the roadmap: a future requirement is to authenticate once and then poll a command repeatedly over a kept-alive connection. An in-process library holds the authenticated session (including the nested jumphost session) and reuses it by opening a new channel per poll — authenticate once, poll many. Shell-out's equivalent is OpenSSH connection multiplexing (`ControlMaster`/`ControlPersist`), which Windows OpenSSH does not support, so that path cannot satisfy the roadmap on the target boxes. `ssh2` would also be in-process but pulls in C deps; `russh` keeps both the persistence story and a clean Windows build.

### Jumphost routing: nested session over a forwarded channel

When a jumphost is given, we open an SSH session to it, open a `direct-tcpip` channel to `remote_host:22`, and run a *second* SSH session over that channel's byte stream — the same mechanism as OpenSSH's `ProxyJump`/`-J`. When no jumphost is given, we connect to the remote host directly. The two paths converge on the same "authenticate, run command, capture stdout" code by abstracting over the underlying stream.

### Authentication: agent first, password fallback

For each hop we try ssh-agent keys first; if the agent is absent or no key authenticates, we prompt for a password (hidden input). The same configured username and the same auth procedure apply to both hops. This mirrors what the user gets from SSH by hand.

### Library surface: one capture call

The library exposes a single entry point taking the connection inputs (remote host, username, command, optional jumphost) and returning the captured stdout as `Result<String, _>`. That is the whole public surface for this cut; the error type stays minimal (a thin enum or a boxed error) since rich error handling is out of scope.

### Crate layout: one package, lib + bin

A single Cargo package with a library target (`src/lib.rs`) and a binary target (`src/main.rs` or `src/bin/`) that depends on it. Minimal structure for a demonstrator; a workspace would be ceremony.

### CLI binary: `clap` args, `arboard` clipboard

The binary parses arguments with `clap`. Invocation is SSH-style: `sshcmd user@host [--jumphost <host>] -- <command...>`. The destination carries the username (`user@host`); the optional jumphost is a bare host that reuses that same username (per the single-username decision); everything after `--` is taken verbatim as the remote command. No port flag for this cut (assume 22). The binary calls the library, then prints the captured output to stdout *and* copies it to the clipboard via `arboard` — a maintained cross-platform clipboard crate that works on Windows.

### Building and verifying

The agent's deliverable is a library and CLI that compile and pass unit tests on Linux. Real-world verification is the user's: local end-to-end runs on this Linux environment against a reachable SSH target, and Windows compilation + testing on a separate machine. The agent does not set up cross-compilation or a local SSH server. The plan therefore includes written **Windows build-and-test instructions** so the user can compile the `.exe` elsewhere during testing.

## Plan

- [x] Scaffold the Cargo package with `lib` + `bin` targets and dependencies (russh, tokio, clap, arboard, rpassword)
- [x] Implement agent-then-password authentication for a single hop
- [x] Implement connection setup: direct to the remote, or a nested session through a jumphost channel
- [x] Implement the public capture call: run the command and return its stdout as a `String`
- [x] Implement CLI argument parsing for `user@host [--jumphost <host>] -- <command...>`
- [x] Print the captured output and copy it to the clipboard
- [x] Unit-test `user@host` destination parsing
- [x] Write Windows build-and-test instructions in the README

## Log

- A `cargo add clap` initially failed to resolve `clap_derive =4.6.1` against a stale local index cache (which listed `clap_derive` only up to 4.6.0). A `cargo remove`/`cargo add` cycle refreshed the index and it resolved; `clap_derive 4.6.1` does exist and clap 4.6.1 builds fine. Not a clap issue — a transient index-staleness artifact. Dependency left as `clap = "4"`.
- Library error type is `anyhow` rather than a bespoke enum — the Approach permitted a boxed error, and `anyhow` keeps the demonstrator minimal while error handling is out of scope.
- Clipboard copy is best-effort: on failure (e.g. a headless machine with no clipboard) the binary prints a warning and still exits 0, since the output has already gone to stdout. Matters for local Linux testing here.
- Host key verification accepts any key (`check_server_key` returns `true`) — noted in code as a security gap to revisit; out of scope for v0.1.0.
- ssh-agent discovery uses `SSH_AUTH_SOCK` (russh `connect_env`). Windows OpenSSH's named-pipe agent isn't found that way, so on Windows the agent step is skipped and it falls back to the password prompt. Acceptable for this cut; flag for the roadmap.
- Correction to the above: `connect_env` is `#[cfg(unix)]` in russh — it does not exist on Windows, so `authentication.rs` as written would not *compile* for Windows, not merely skip the agent. Fixed: the agent connection is now cfg-gated — `connect_env` on Unix, `connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")` on Windows — with the identity-offering logic made generic over the agent stream so both platforms share it. The Unix branch is built and tested here; the Windows branch compiles only when cross-built (verified by the user's build).
- The Approach's "pure Rust → no C dependencies" premise was wrong: russh 0.61's default crypto backend is `aws-lc-rs`, a C library built via cmake. Switched russh to its `ring` backend (`default-features = false`, `features = ["flate2", "ring", "rsa"]`) — ring is C/asm but needs no cmake and cross-compiles to `x86_64-pc-windows-gnu` reliably. Verified: aws-lc-sys leaves the tree, Linux build + tests stay green.
- Cross-compile to `x86_64-pc-windows-gnu` was attempted here. With ring the only missing piece is the `mingw-w64` toolchain (build stops at `x86_64-w64-mingw32-dlltool`); no cmake/nasm needed. The user then ran `cargo build --release --target x86_64-pc-windows-gnu` with mingw installed and it built successfully — confirming the ring backend, the final link, and the `#[cfg(windows)]` agent branch all compile for Windows. A live run against real jumphost/remote infrastructure was not reported.
- Inspected the produced `sshcmd.exe` with `objdump -p`: it imports only standard Windows system DLLs (ntdll, kernel32, user32, ws2_32, bcryptprimitives, advapi32, gdi32, shell32, oleaut32, userenv, legacy `msvcrt`, api-ms-win-core-*). No mingw runtime DLLs and no Visual C++ Redistributable — the binary is standalone, needs no installation and no admin rights to run. README runtime note tightened accordingly.

## Conclusion

Delivered at v0.1.0: the `sshcmd` library (`capture`), the CLI demonstrator (print + clipboard), and a verified Linux→Windows cross-compile path.

Two Approach premises were corrected mid-build:

- "Pure Rust, no C deps" was false — russh 0.61 needs a C crypto backend (aws-lc-rs by default). Switched to `ring` (no cmake, cross-compiles cleanly).
- Agent auth needed platform-gating — russh's `connect_env` is Unix-only, so Windows now uses a named-pipe connection (`\\.\pipe\openssh-ssh-agent`).

README corrected (the "pure Rust" claim) and given cross-compile steps. The Windows `.exe` built successfully (user-confirmed); a live run against real jumphost/remote infrastructure remains to be done.
