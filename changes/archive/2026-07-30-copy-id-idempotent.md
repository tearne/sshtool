# copy-id idempotency

**Mode:** Formal

## Intent

Running `copy-id` more than once against the same host appends a fresh copy of the agent's public key to the remote `authorized_keys` every time, because the install step unconditionally appends the line. A user who re-runs `copy-id` — to re-verify, after a failed hop, or out of habit — accumulates duplicate key entries, leaving `authorized_keys` messier on each run.

`copy-id` should be idempotent: installing a key that is already present should leave `authorized_keys` unchanged and still report success. The fix must hold on the restricted shells (rbash) the current install command was written to support — the reason it uses `printf | tee` rather than output redirection.

## Approach

### Guard the append with a whole-line `grep`

The install command gains a `grep` guard: append only when the key line is not already present. `grep -qxF <key> .ssh/authorized_keys` tests for an exact, whole-line, fixed-string match; on a hit the `||` short-circuits and the append is skipped. This keeps success (exit 0) whether or not the key was already there.

### `touch` the file so `grep` never errors on rbash

`grep` against a missing `authorized_keys` errors to stderr, and suppressing that (`2>/dev/null`) is a redirection rbash blocks. Instead the command `touch`es the file first — no redirection, and `umask 077` still gives it correct permissions — so `grep` always has a file to read.

### Match the full stored line, comment included

The guard matches the exact string the tool writes (`type base64 comment`). A pre-existing entry differing only in trailing comment is treated as absent and appended — accepted, because the alternative (parsing out the key blob) adds shell complexity for a case that barely arises when the same agent key is reinstalled.

## Plan

- [x] Add a `touch` + `grep -qxF … ||` guard around the `printf | tee -a` in `install_on_handle`, and update its comment to cover the guard and the rbash-redirection constraint.
- [x] Verify a second `copy-id` run leaves a single key line in `authorized_keys` and still reports success; Linux build and Windows cross-check pass.

## Conclusion

Completed as planned; no deviations. Ships as 0.10.1 (patch).

Proposed changelog entry (0.10.1):

- Fix: `copy-id` is now idempotent. The install step guards the append with `grep -qxF`, so re-running `copy-id` against a host no longer adds duplicate copies of the key to `authorized_keys`. Still works on restricted shells (rbash) — the guard uses no output redirection.
