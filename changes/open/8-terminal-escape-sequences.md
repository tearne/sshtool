# Terminal escape sequences

**Mode:** _(to be set after Intent approval)_

## Intent

Once `sshtool` has an interactive console (PTY passthrough), it should recognise OpenSSH-style escape sequences typed at the start of a line — at minimum `~.` to drop a wedged connection, with `~?` (help) and `~~` (a literal tilde), and room to grow (suspend, list/add forwards, rekey). OpenSSH implements these client-side and intercepts them locally before sending input to the remote; `russh` does not provide them, so `sshtool` must handle them itself. The escape character should be configurable (OpenSSH uses `-e`).

`~.` must mean *quit entirely*: under `-r` (reconnect) it has to break out of the reconnect loop and exit cleanly, not be mistaken for a dropped connection that triggers a reconnect. Distinguishing a user-requested quit from a network drop is the key interaction with existing behaviour.

The interactive console now exists, so this is built standalone rather than folded into a future console change.

## Approach

### An `escape` module holds the interception

A new module owns a stateful processor; `pump` feeds each locally-typed stdin chunk through it and forwards the bytes it returns, acting on any signalled command. This keeps the select loop readable and the state in one place. It sits only on the interactive stdin path, so streamed commands and `-N` never see escapes — as in ssh, which offers them only with a tty.

### Line-oriented state machine, matching OpenSSH

The escape character is recognised only at line start — session start or after CR/LF, never mid-word. On seeing it the processor holds the character until the next byte disambiguates: a command byte, a literal escape, or anything else (both bytes forwarded).

### Three commands, room to grow

`~.` quit, `~?` help, `~~` a literal escape character. Unrecognised sequences pass through untouched. Suspend / forwards / rekey are left for later.

### Quit is a distinct session outcome

`run_once` returns a typed outcome — exited / dropped / quit — replacing today's `Result<u32>` where any non-exit is a drop. `supervise` treats quit as a clean stop that bypasses reconnect. This is the Intent's key requirement: `~.` under `-r` must end the program, not restart it.

### `-e` configures the escape character

A new `-e` flag sets the escape character (default `~`) or `none` to disable interception, matching ssh. Control-character (`^X`) notation is out of scope.

### Help prints locally, raw-mode-safe

`~?` writes the escape list to stderr with `\r\n` endings, since stdout carries the remote stream and the terminal is raw.

## Unresolved

- Exit code for `~.`: propose `0` (a deliberate user quit, not an error). OpenSSH returns 255. Which do you want?
- `-e` accepting only a single character or `none`, skipping OpenSSH's `^X` control-character notation — acceptable for this change?
