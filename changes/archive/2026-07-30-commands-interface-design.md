# Commands interface design: flags vs subcommands

**Mode:** Explore

## Intent

The mode operations (`keygen`, `copy-id`, `install`) are currently exposed as flags (`--keygen`, `--copy-id`, `--install`) on a single parser. This was chosen to avoid hostname collisions — since the default invocation is `sshtool <host>`, a subcommand named `keygen`/`copy-id`/`install` could shadow a real host or config alias with that name.

But the flag design has costs that surfaced in testing: there is a single flat `--help` with no per-command help (`sshtool copy-id --help`), which is what a user reaches for by reflex; and clap's generic `conflicts_with` errors read awkwardly — e.g. `sshtool --install ranch-1117` reports "`--install` cannot be used with `[DESTINATION]`" while the usage line beneath still shows `[DESTINATION]`, which looks self-contradictory.

This change is a decision, not an implementation: weigh the flag approach (collision-proof, but flat help and clumsy errors) against proper subcommands (conventional, scoped per-command help, tailored errors — at the cost of reintroducing the hostname-collision class and needing a default-subcommand pattern, shared-flag handling, and `trailing_var_arg` care). The outcome settles which interface `sshtool` commits to, and thus whether the friendlier error messages are worth hand-writing or become moot.

## Approach

The decision is settled: move to subcommands. A clap prototype on the shipped 4.6 line confirmed the default-connect pattern works cleanly and clarified the true costs, which are acceptable.

### Subcommands, with connect as the unnamed default

`keygen`, `copy-id`, and `install` become clap subcommands; running `sshtool [user@]host …` with no subcommand takes the connect path. This gives conventional per-command `--help`, lets each op own and validate its own arguments, and deletes the `cfg_attr(… conflicts_with_all = […])` machinery the flag design needed. That machinery removal is a net simplification, which was a stated goal.

### Two narrow collisions are accepted, escaped by `--`

A bare destination equal to the exact word `keygen`/`copy-id`/`install` routes to the subcommand (qualify as `user@keygen` or an FQDN to avoid). More notably, the first word of a remote command matching a subcommand name is hijacked — `sshtool host keygen` runs the local subcommand, so `sshtool host -- keygen` is required. The `--` price is accepted as reasonable, even desirable as an explicit boundary between local dispatch and remote command.

### Optional subcommand beside top-level connect args

Modelled as `#[command(subcommand)] action: Option<Action>` alongside a top-level `destination: Option<String>`, the trailing `command`, and the connection flags — no manual `args[1]` pre-routing. `copy-id` carries its own `destination` and `-J`; `keygen`/`install` take none. `install` stays a Windows-gated variant.

Connect is also reachable as an explicit `connect` subcommand, listed in help for discoverability, in addition to the unnamed default — so both `sshtool user@host` and `sshtool connect user@host` work. The connection argument set is defined once and shared between the default path and the `connect` subcommand rather than duplicated.

The trailing remote command's value is named `REMOTE_COMMAND` so the help usage line reads clearly instead of showing two `COMMAND` slots.

## Plan

**Topics**

- A shared connection-args struct (`destination`, `-J`, `-t`, `-N`, `-r`, `-L`, `-v`, and the `REMOTE_COMMAND` trailing arg) flattened into both the top-level default path and the `connect` subcommand.
- An `Action` subcommand enum — `connect`, `keygen`, `copy-id` (own `destination` + `-J`), and Windows-gated `install` — replacing the mode flags and their `conflicts_with` attributes.
- `main` dispatch on `Option<Action>`: `None` and `Connect` take the connect path, the rest run their operation.
- Help and usage check: all subcommands listed, each has its own `--help`, and the connect usage shows a single clean `REMOTE_COMMAND`.

**Done when** the three management ops run as subcommands with scoped `--help`; `sshtool host`, `sshtool connect host`, and `sshtool host -- keygen` all behave correctly; and Linux build plus Windows cross-build pass.

## Log

- Restructured `main.rs` into `Cli { connect: ConnectArgs (flattened), action: Option<Action> }`. `ConnectArgs` is shared between the default path and the `connect` subcommand; `Action` covers `connect`/`keygen`/`copy-id`/(Windows) `install`. All `conflicts_with` machinery removed.
- Verified: `sshtool host`, `sshtool connect host`, and `sshtool host -- keygen` (remote) all behave; `sshtool host keygen` runs the local subcommand as designed. `copy-id`/`keygen` have scoped `--help`. Linux build and Windows cross-check pass.
- Set `subcommand_value_name = "SUBCOMMAND"` so the usage line reads `[DESTINATION] [REMOTE_COMMAND]... [SUBCOMMAND]` instead of a second generic `[COMMAND]`.
- Took the chance to reword the `-r` help to say pair it with a remote multiplexer *command* (e.g. `tmux attach`), clarifying the point raised earlier this session.

## Conclusion

Completed as planned; the Log already records the two in-flight refinements (`subcommand_value_name`, `-r` help wording). Version bumped 0.9.6 → 0.10.0 (minor). This change reverses the interface direction taken in the 0.9.5 changelog entry, which introduced the mode flags.

Proposed changelog entry (0.10.0):

- The management operations are now clap subcommands — `sshtool keygen`, `sshtool copy-id user@host [-J jumphost]`, and (Windows) `sshtool install` — replacing the `--keygen` / `--copy-id` / `--install` flags introduced in 0.9.5. Each subcommand owns its arguments and has its own `--help`. Connecting is unchanged: `sshtool [user@]host …` still connects by default, and `sshtool connect …` is an explicit equivalent. A remote command whose first word matches a subcommand name must be separated with `--` (e.g. `sshtool host -- keygen`).
