# Terminal escape sequences

**Mode:** _(to be set after Intent approval)_

## Intent

Once `sshtool` has an interactive console (PTY passthrough), it should recognise OpenSSH-style escape sequences typed at the start of a line — at minimum `~.` to drop a wedged connection, with `~?` (help) and `~~` (a literal tilde), and room to grow (suspend, list/add forwards, rekey). OpenSSH implements these client-side and intercepts them locally before sending input to the remote; `russh` does not provide them, so `sshtool` must handle them itself. The escape character should be configurable (OpenSSH uses `-e`).

Likely folded into the interactive-console change when that is planned, rather than built standalone.
