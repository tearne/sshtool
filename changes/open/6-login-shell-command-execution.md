# Login-shell command execution

**Mode:** _(to be set after Intent approval)_

## Intent

A command run via `sshtool host cmd` (or `-t host cmd`) executes in a non-login, non-interactive remote shell, which doesn't source the user's profile/rc files. So `PATH` entries built there — e.g. `~/.cargo/bin` or `~/.local/bin`, where tools like `zellij`/`tmux` often live — are absent, and `sshtool -t host zellij a` fails with "command not found" even though the same command works in an interactive `sshtool host` shell.

This change adds an opt-in way to run the command through a login shell (e.g. `$SHELL -lc "<command>"`) so it inherits the same environment the interactive shell has. The default stays faithful to `ssh` (non-login); the login behaviour is a flag, because login shells also emit motd/banners and run profile side effects.
