# Resolve explicit -J jumphost through ssh config

**Mode:** Formal

## Intent

A jumphost passed explicitly with `-J <alias>` is used literally — `sshtool` connects to the raw string rather than resolving it through `~/.ssh/config`. So `sshtool -J jh2 ranch-1127 -- pwd` fails with "No such host is known" because `jh2` is a config alias whose real `HostName` (`ssh2.int.sce.network`) is never looked up. Meanwhile the same alias works when it comes from a host's `ProxyJump` in config, because that path already resolves `HostName`. `ssh -J jh2 …` resolves the alias; `sshtool` should match it.

An explicit `-J` alias should honour the same `~/.ssh/config` settings a config-derived `ProxyJump` does — at least `HostName` and `User` — so the two ways of specifying a jumphost behave consistently.

## Approach

### One shared jumphost resolver for both paths

Extract a single function that turns a `[user@]host` jump spec into a resolved `Endpoint` — `parse_endpoint`, then `ssh_config::resolve` on the host, applying `HostName`. Both the CLI `-J` path in `target_from` and the config `ProxyJump` path in `resolve_proxy_jump` call it. The bug exists because only the config path resolves today; a shared resolver makes the divergence structurally impossible rather than fixing one call site.

### Jumphost username falls back to config `User`

The resolved endpoint's username is the one from the spec, else the config's `User`. This closes the second half of the asymmetry the Intent names, and matches how `ssh` treats a jump alias. It is a slight behaviour change for the config `ProxyJump` path, which currently ignores config `User` — that convergence is the point.

### Jumphost port and multi-hop stay out of scope

The jumphost is modelled as an `Endpoint` with no port, and the connection layer always dials it on port 22 — honouring `Port` for a jump is a separate, pre-existing gap. Multi-hop chains are likewise excluded: the whole tool is single-jump today (config `ProxyJump` already rejects multiple hops), and supporting a chain means reworking the connection layer's data model and session-lifetime handling, well beyond this config-resolution fix. `-J` keeps rejecting comma-separated multi-hop, matching the config path.

## Plan

- [x] Add a shared jumphost resolver in `main.rs` that turns a `[user@]host` spec into an `Endpoint`, applying config `HostName` and falling back to config `User` for the username
- [x] Route the CLI `-J` path in `target_from` through the shared resolver
- [x] Route `resolve_proxy_jump`'s single hop through the shared resolver
- [x] Verify Linux build and Windows cross-build pass

## Log

- Implemented as planned: a shared `resolve_jumphost` in `main.rs` now backs both the CLI `-J` path and `resolve_proxy_jump`'s single hop, applying config `HostName` and falling back to config `User`.
- Verified behaviourally with a temp `~/.ssh/config`: `-J <alias>` now resolves to the alias's `HostName` (the connection error names the resolved host), while a raw `-J host` stays literal. Reproduces and fixes the reported `-J jh2` failure. Linux build and Windows cross-check pass; version bumped to 0.9.6.

## Conclusion

Completed as planned. Alongside fixing explicit `-J`, the config `ProxyJump` path now also honours config `User` for the jumphost (it previously dropped it) — the intended convergence, confirmed during planning. No other behaviour changed; jumphost port and multi-hop remain out of scope (the latter captured as `multi-hop-jumphost.md`). Verified on the Windows laptop: `sshtool -J jh2 ranch-1127 -- pwd` now connects.

Proposed changelog entry (0.9.6):

> Fix: an explicit `-J <alias>` jumphost is now resolved through `~/.ssh/config` (honouring the alias's `HostName` and `User`), matching how a config `ProxyJump` alias already resolved. `sshtool -J jh2 host` no longer fails with "No such host is known" when `jh2` is a config alias.
