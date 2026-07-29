# Commands interface design: flags vs subcommands

**Mode:** _(to be set after Intent approval)_

## Intent

The mode operations (`keygen`, `copy-id`, `install`) are currently exposed as flags (`--keygen`, `--copy-id`, `--install`) on a single parser. This was chosen to avoid hostname collisions — since the default invocation is `sshtool <host>`, a subcommand named `keygen`/`copy-id`/`install` could shadow a real host or config alias with that name.

But the flag design has costs that surfaced in testing: there is a single flat `--help` with no per-command help (`sshtool copy-id --help`), which is what a user reaches for by reflex; and clap's generic `conflicts_with` errors read awkwardly — e.g. `sshtool --install ranch-1117` reports "`--install` cannot be used with `[DESTINATION]`" while the usage line beneath still shows `[DESTINATION]`, which looks self-contradictory.

This change is a decision, not an implementation: weigh the flag approach (collision-proof, but flat help and clumsy errors) against proper subcommands (conventional, scoped per-command help, tailored errors — at the cost of reintroducing the hostname-collision class and needing a default-subcommand pattern, shared-flag handling, and `trailing_var_arg` care). The outcome settles which interface `sshtool` commits to, and thus whether the friendlier error messages are worth hand-writing or become moot.
