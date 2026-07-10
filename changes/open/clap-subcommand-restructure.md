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

- [ ] Rewrite `Args` in `main.rs`: make `destination` `Option<String>`, add `--keygen`, `--copy-id`, and `--install` (cfg-gated) boolean flags with `conflicts_with` attributes
- [ ] Remove the manual `match args.get(1)` pre-routing block
- [ ] Update `main` to branch on the mode flags
- [ ] Merge `target_from_copy_id` into `target_from` (destination now optional; callers that require it validate after)
- [ ] Verify `--help` output lists all flags cleanly
- [ ] Verify `cargo build` and Windows cross-build pass
