# Support for ~/.ssh/config

**Mode:** Formal

## Intent

`sshtool` should read connection settings from the user's OpenSSH client config (`~/.ssh/config`) so that an invocation like `sshtool myalias -- …` honours what the user already has set up for `ssh` — host aliases and their `HostName`, `User`, `Port`, `ProxyJump`, `IdentityFile`, and similar — instead of forcing every value onto the command line. This matches user expectations and reduces flags, especially as jumphosts and forwarding grow.

Scope (which directives to honour, precedence against explicit flags) to be worked out when planned.

## Approach

### Use `ssh2-config` for parsing

The `ssh2-config` crate (v0.7.1) parses `~/.ssh/config` into a `HostParams` struct, performing `Host`-pattern matching and `Match` block evaluation. Its `HostParams` exposes exactly the fields we need: `host_name`, `user`, `port`, `proxy_jump`, `identity_file`.

Writing our own parser would be substantial — OpenSSH config has complex pattern matching, `Include` directives, and case-insensitive token handling. The crate is MIT-licensed and already tracks the `ssh2-rs` project's needs, making it a well-exercised choice.

### Directives to honour

Four directives cover the Intent:

- `HostName` — the real hostname to connect to (the CLI argument becomes the alias)
- `User` — default username when none is given as `user@host`
- `Port` — port to connect on (currently hardcoded to 22)
- `ProxyJump` — jumphost to route through; if the config lists more than one hop, exit with an error explaining that chained jumps are not currently supported

`IdentityFile` is explicitly deferred — it requires extending the authenticator to load key files directly, which is a separate concern.

### Explicit CLI flags take precedence over config

If the user supplies `user@host` with a username, that username wins. If `-J` is given, that jumphost wins. Port and `IdentityFile` have no current CLI flag so the config value applies unconditionally when present.

This follows standard SSH semantics: command-line arguments are highest priority, then config file, then defaults.

### Config resolution lives in a new `ssh_config` module

A new `src/ssh_config.rs` module exposes a single function that accepts a hostname (the raw CLI argument) and returns resolved `HostParams` (or a sensible default if the config file is absent or the host matches nothing). `main.rs` calls this before constructing `Target`, merging config values where no CLI value was provided.

The `connection` module's hardcoded `SSH_PORT` constant is replaced with the resolved port.

### `Target` gains an optional `port` field

Currently `Target` only carries `username`, `host`, and `jumphost`. Port resolution requires threading it through. Adding `port: u16` to `Target` with a default of 22 is the minimal change; `connection` reads it when opening the TCP connection.

## Plan

- [x] Add `ssh2-config` to `Cargo.toml`
- [x] Add `src/ssh_config.rs` with a `resolve(host: &str) -> HostParams` function that loads `~/.ssh/config` and returns matched params (empty params if file absent or no match)
- [x] Register `ssh_config` module in `lib.rs`
- [x] Add `port: u16` to `Target` in `lib.rs`
- [x] Update `connection` to use `target.port` instead of the hardcoded `SSH_PORT` constant
- [x] Update `main.rs`: call `ssh_config::resolve` after parsing args, then merge config values into `Target` — `HostName` → `host`, `User` → `username` (if no `user@` on CLI), `Port` → `port`, `ProxyJump` → `jumphost` (if no `-J` on CLI; error if more than one hop)
- [x] Update `copy-id` path in `main.rs` to apply the same config resolution
- [x] Verify compilation and smoke-test against a local `~/.ssh/config` alias

## Conclusion

Completed. Two bugs found during testing and fixed before wrapping up: (1) `ssh_config::resolve` was called with the full `user@host` string rather than the bare host, causing config lookup to silently fail when a username was present; (2) `RawMode::enter()` was called after `start()` in the non-reconnecting path, so the first keystrokes were sent in cooked mode and not echoed by the remote PTY. Both are now fixed. The custom parser (in place of `ssh2-config`) and the ProxyJump alias resolution fix are recorded in the Log.

## Log

- Decided against `ssh2-config` crate after discovering its build dependency on `git2` → `openssl-sys`, which requires system libssl headers not available in this environment. Replaced with a self-contained minimal parser in `src/ssh_config.rs` covering Host pattern matching (including `*`, `?`, `!` negation) and the four directives in scope. 6 unit tests cover the parser.
- Windows bug: `HOME` is not set on Windows (PowerShell uses `USERPROFILE`). Fixed `config_path()` to fall back to `USERPROFILE`.
- ProxyJump alias not resolved: the jumphost name from `ProxyJump` was used as-is, but it can itself be a `Host` alias in the config. Fixed `resolve_proxy_jump` to run the jumphost name through `ssh_config::resolve` to expand its `HostName`.
