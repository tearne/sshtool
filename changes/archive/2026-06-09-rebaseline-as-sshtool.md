# Rebaseline as sshtool

**Mode:** Formal

## Intent

`sshcmd` began as a one-shot demonstrator — run a command, capture its output, copy it to the clipboard. The project is now becoming a general, SSH-shaped tool driven by flags: interactive shells, port forwarding, and reconnection are all coming.

This change resets the foundation as **`sshtool`**: rename the crate and binary, and drop the clipboard capture (a demo crutch) so a run command streams its output like `ssh`, ready to pipe or redirect (on Windows, `… | clip` still reaches the clipboard). Jumphost routing, agent-then-password auth, and the `-J`/`-V` flags stay.

## Approach

### Rename to `sshtool`

Rename the Cargo package, the library crate, and the binary from `sshcmd` to `sshtool`, and update the internal `use sshcmd::` and the README/changelog branding. The working directory keeps its current name — renaming it is git/path churn left to the user.

### Drop the clipboard

Remove the `arboard` dependency and the copy-to-clipboard path entirely. The "get the text onto the clipboard" need is served by the OS: pipe the output (`… | clip` on Windows, `… | xclip`/`pbcopy` elsewhere).

### Stream output and propagate the exit status

Replace the capture-into-`String` core with a streaming run. The remote command's stdout and stderr are written to the local stdout/stderr as the bytes arrive, and the process exits with the remote command's exit status — matching `ssh`. The public entry point changes from `capture(&Target) -> Result<String>` to `run(&Target) -> Result<u32>` (the exit code). This streaming shape is also the foundation interactive mode will extend.

Local stdin is not forwarded in this change — output streaming only. Forwarding stdin is deferred and captured as a separate proposal.

### Errors vs. remote exit codes

Connection and authentication failures stay hard errors (exit 1 via `anyhow`). A command that runs but exits non-zero is not an error — its code is propagated as `sshtool`'s exit code, as `ssh` does.

## Plan

- [x] Rename the Cargo package, library crate, and binary to `sshtool`
- [x] Remove the `arboard` dependency and the clipboard handling
- [x] Stream stdout and stderr live and return the remote exit status, replacing `capture` with `run`
- [x] Exit the binary with the remote command's status
- [x] Update the README and changelog branding and drop clipboard references
- [x] Make the leading `--` optional (SSH-style trailing args) — added during review

## Log

- Streaming to the local terminal needed tokio's `io-std` feature for `stdout()`/`stderr()`; added it.
- Historical changelog entries (0.1–0.3) are left as written — they accurately record the `sshcmd` era. The rename is recorded by the new 0.4.0 entry drafted at Conclusion.
- `cargo build` doesn't delete a renamed crate's old binary; the stale `sshcmd`/`sshcmd.exe` artifacts were removed by hand to avoid confusion.
- During review the user found the required `--` awkward; relaxed it to SSH-style trailing args (clap `trailing_var_arg`), so `sshtool host cmd…` works and `--` is optional. A command is still required until the interactive-shell change adds the no-command path. Bumped to 0.4.1.

## Conclusion

Completed. `sshcmd` is now `sshtool`; clipboard capture is gone; a command streams its stdout/stderr live and the process exits with the remote status — the SSH-shaped core the flag-based features will build on. During review the required `--` was relaxed to SSH-style trailing args (a command is still required until the interactive shell lands). Shipped at 0.4.1. Dropping `arboard` shrank the Windows binary from ~14 MB to ~9.9 MB. Historical changelog entries are left intact as the `sshcmd`-era record.
