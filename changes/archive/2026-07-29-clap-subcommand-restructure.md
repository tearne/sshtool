# Clap subcommand restructure

**Mode:** Formal

## Intent

Subcommands (`keygen`, `copy-id`, `install`) are currently routed via manual `args[1]` matching before clap sees the arguments, making them invisible to the help system. Restructure `main.rs` to use clap subcommands properly so all commands appear in `--help` with their descriptions, while keeping the default SSH invocation (`sshtool user@host`) working without a subcommand.

## Approach

### Replace subcommands with mode flags

`--keygen`, `--copy-id`, and `--install` become boolean flags on the single `Args` struct. This avoids any collision between subcommand names and hostnames, keeps a single parser, and surfaces everything in `--help` naturally.

`destination` becomes `Option<String>` — not required for `--keygen` or `--install`.

### Use `conflicts_with` for mutual exclusion

Clap's `conflicts_with` / `conflicts_with_all` attributes encode incompatible flag combinations declaratively. `--keygen` and `--install` conflict with `destination`, `-J`, `-t`, `-N`, `-r`, `-L`, `-v`, and each other. Clap surfaces these in `--help` and rejects invalid combinations before `main` runs — no manual validation needed.

`--copy-id` conflicts with `-t`, `-N`, `-r`, `-L` but requires `destination` and allows `-J`.

### `--install` is cfg-gated

The flag only appears on Windows, matching the current behaviour.

### The manual pre-routing block is removed

The `match args.get(1)` block is deleted. `main` parses once with clap and branches on which mode flag is set (or none, for the default SSH path). `CopyIdArgs` is absorbed into the unified struct — `target_from_copy_id` merges with `target_from`.

## Plan

- [x] Rewrite `Args` in `main.rs`: make `destination` `Option<String>`, add `--keygen`, `--copy-id`, and `--install` (cfg-gated) boolean flags with `conflicts_with` attributes
- [x] Remove the manual `match args.get(1)` pre-routing block
- [x] Update `main` to branch on the mode flags
- [x] Merge `target_from_copy_id` into `target_from` (destination now optional; callers that require it validate after)
- [x] Verify `--help` output lists all flags cleanly
- [x] Verify `cargo build` and Windows cross-build pass

## Log

- Resumed the build and found tasks 1–4 already implemented and committed as `3464639 "wip clap restructure"`; remaining work was verification (tasks 5–6).
- Verification caught a real bug: `destination` and `keygen` listed the cfg-gated `install` in `conflicts_with_all`, so on non-Windows builds clap panicked at startup ("Argument or group 'install' ... does not exist"). Fixed by gating the `install` reference with `cfg_attr` — Windows keeps the full conflict set, non-Windows omits `install`. The `destination`↔`keygen` conflict holds on both. Linux `--help` runs cleanly; Windows cross-check passes.

## Conclusion

Completed as planned; the flag-based design was retained after confirming the hostname-collision risk is the deciding reason to prefer flags over subcommands.

Deviations and extras beyond the plan:

- Fixed the cfg-gated `install` conflict panic on non-Windows (see Log) — a latent bug in the committed `wip` work, not a plan step.
- Version bumped to 0.9.5 and verified via full Linux build and `build_win.sh` release build.
- Touched an unrelated stale doc comment in `interactive.rs` (Windows resize described as "handled separately, later" — now implemented) that rode along during this session.

Two design questions surfaced during testing and were spun off rather than folded in: whether flags-vs-subcommands is the right long-term interface (`commands-interface-design.md`), and a functional bug where explicit `-J <alias>` isn't resolved through ssh config (`resolve-cli-jumphost-from-config.md`).

Proposed changelog entry (0.9.5):

> `--keygen`, `--copy-id`, and `--install` (Windows) are now first-class flags listed in `--help`, replacing the previous hidden subcommand-style routing. The default `sshtool user@host` invocation is unchanged. Flags were chosen over subcommands to avoid collisions between command names and hostnames / ssh-config aliases.
