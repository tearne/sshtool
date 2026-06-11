# Clap subcommand restructure

**Mode:** _(to be set after Intent approval)_

## Intent

Subcommands (`keygen`, `copy-id`, `install`) are currently routed via manual `args[1]` matching before clap sees the arguments, making them invisible to the help system. Restructure `main.rs` to use clap subcommands properly so all commands appear in `--help` with their descriptions, while keeping the default SSH invocation (`sshtool user@host`) working without a subcommand.
