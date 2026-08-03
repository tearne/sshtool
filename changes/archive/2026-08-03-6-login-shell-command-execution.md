# Login-shell command execution

**Mode:** Formal

## Intent

A command run via `sshtool host cmd` (or `-t host cmd`) executes in a non-login, non-interactive remote shell, which doesn't source the user's profile/rc files. So `PATH` entries built there — e.g. `~/.cargo/bin` or `~/.local/bin`, where tools like `zellij`/`tmux` often live — are absent, and `sshtool -t host zellij a` fails with "command not found" even though the same command works in an interactive `sshtool host` shell.

When a command runs over a PTY — via `-t`, or the reconnecting `-r` used to attach a multiplexer — it should run through a login shell so it inherits that same environment. The plain streamed path (`sshtool host cmd` with no PTY) stays faithful to `ssh` (non-login), where profile banners and side-effects would otherwise pollute captured output.

## Approach

### Login shell only on the PTY command path

A command run over a PTY (`-t`, or `-r` attaching a multiplexer) is wrapped in a login shell; the plain streamed command path stays non-login, exactly as `ssh` does. The split follows what the path is for: a PTY command is an interactive-ish program that wants the user's full environment, whereas the streamed path's stdout is often captured, and login profiles emit banners and side-effects that would corrupt it.

### Wrap by re-invoking the user's login shell

The wrapped form is `${SHELL:-bash} -lc '<command>'`: `$SHELL` honours whatever login shell the account uses, falling back to `bash` when it is unset; `-l` is what sources the profile where `PATH` (and thus `zellij`/`tmux`) is built. The original command is single-quoted with the `'\''` escape so its metacharacters reach the login shell intact.

### Transform the command where the foreground is chosen

The wrapping is a pure transformation of the command string applied in `main` at the point the PTY-command foreground is selected, so the finalised command flows unchanged into the exec path. The `session`, `command`, and `interactive` modules stay unaware of login shells, and no new CLI surface is added.

### Note the `-t` implication in `-r`'s help

`-r` already forces a PTY for a command (it exists to attach a multiplexer), so it implies `-t`. That coupling is currently undocumented; the `-r` help gains a note, since the login-shell behaviour rides on the same PTY-command path and the implication should be visible to the user.

## Plan

- [x] Wrap the PTY-command foreground (`-t`/`-r`) in `${SHELL:-bash} -lc '<command>'`, single-quote-escaping the command, where `foreground_for` builds `Interactive(Some(_))`.
- [x] Extend the `-r` help to state it forces a PTY (implies `-t`).
- [x] Test the login-shell wrap's single-quote escaping.

## Log

- Hand-back review found `-t`'s help said nothing about the login shell; added it there (the canonical PTY flag), with `-r` deferring to `-t`. Patch bump to 0.12.1.

## Conclusion

Completed as planned. The only additions beyond the three tasks were help-text refinements surfaced during hand-back (see Log): `-t` now mentions the login shell, and `-r`'s text was tightened. No `map.md` in this project. Ships as 0.12.1.
