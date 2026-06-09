# Jumphost accepts the user@host form

**Mode:** Formal

## Intent

Specifying a jumphost the natural SSH way currently fails. The tool treats the whole `--jumphost` value as a hostname, so `--jumphost o@192.168.88.180` tries to resolve the literal string `o@192.168.88.180` and dies with an opaque "Name or service not known". People reasonably expect `user@host` to work for a jumphost, because that is how every SSH tool names a target.

This change aligns the tool with normal SSH. The jumphost is given by a `-J` flag — matching SSH's `ProxyJump` — that accepts `[user@]host`. The jumphost may carry its own username, relaxing v0.1.0's assumption that one username serves both hops; when no jumphost username is given it falls back to the destination's, as SSH does. A genuine resolution failure should also read legibly — naming the host that couldn't be found and hinting at the likely cause — rather than surfacing a bare lookup error.

## Approach

### Flag: `-J [user@]host`

Replace `--jumphost <host>` with `-J`, matching SSH's `ProxyJump` short flag, accepting `[user@]host`. Keep `--jumphost` as a long alias so `--help` reads clearly.

### One `[user@]host` parser, username optional

Generalise the current `user@host` destination parser into a single endpoint parser returning an optional username and a required host. The destination still requires a username (error if absent); the jumphost does not. One parser keeps both inputs consistent and is the natural home for the "host must be present" rule.

### Jumphost username falls back in the connection layer

A jumphost given without a username reuses the destination's. This is resolved where the connection is built, since that is the only place both usernames are in hand — keeping the fallback policy next to the code that applies it, and out of the CLI.

### Library surface carries the jumphost username

`Target`'s `jumphost: Option<String>` becomes an endpoint carrying an optional username and host. This is a breaking change to the v0.1.0 API, acceptable pre-1.0 and required for the feature.

### Cleaner connection failure

With the username parsed off, the host named in a connection error is already clean (`192.168.88.180`, not `o@192.168.88.180`). On any connection failure, append a generic hint that the host may be unknown or unreachable — simple, and no fragile matching on the error chain.

## Plan

- [x] Generalise endpoint parsing to `[user@]host` with an optional username
- [x] Carry the jumphost as an endpoint (optional username + host) in `Target`
- [x] Resolve the jumphost username (falling back to the destination's) and authenticate the jump hop with it
- [x] Replace `--jumphost <host>` with `-J [user@]host`, keeping `--jumphost` as a long alias
- [x] Append a generic host-unknown-or-unreachable hint on connection failure
- [x] Update endpoint-parsing unit tests for the optional username
- [x] Update the README for `-J` and per-hop usernames

## Log

- The parser generalisation is a wider breaking API change than just the flag: the `destination` module became `endpoint`, `parse_destination`/`Destination` became `parse_endpoint`/`Endpoint`, and `Target.jumphost` is now `Option<Endpoint>`. All intended; noted for the changelog.
- Verified the original failure reproduces fixed: `sshcmd -J jumpuser@nonexistent.invalid alice@remote.invalid -- ls` now reports `connecting to nonexistent.invalid (is the host name correct and reachable?)` — bare host, username parsed off, plus the hint — instead of trying to resolve the literal `jumpuser@…` string.
- Cross-compiled the v0.2.0 Windows binary here (`cargo build --release --target x86_64-pc-windows-gnu`) — builds clean, including the `#[cfg(windows)]` agent branch, and `objdump` confirms it still imports only standard Windows system DLLs (standalone).

## Conclusion

Completed as planned. The jumphost now takes `-J [user@]host` (with `--jumphost` kept as an alias) and authenticates with its own username, falling back to the destination's; connection errors name the bare host with a generic reachability hint.

Beyond the flag, this was a breaking API change — `destination`→`endpoint` module, `parse_destination`/`Destination`→`parse_endpoint`/`Endpoint`, and `Target.jumphost` now `Option<Endpoint>` — hence the 0.2.0 minor bump. README updated; the Windows binary was cross-built here and confirmed standalone; the user confirmed a real-host run works.
