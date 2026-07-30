# copy-id idempotency

**Mode:** TBD — proposing after Intent approval

## Intent

Running `copy-id` more than once against the same host appends a fresh copy of the agent's public key to the remote `authorized_keys` every time, because the install step unconditionally appends the line. A user who re-runs `copy-id` — to re-verify, after a failed hop, or out of habit — accumulates duplicate key entries, leaving `authorized_keys` messier on each run.

`copy-id` should be idempotent: installing a key that is already present should leave `authorized_keys` unchanged and still report success. The fix must hold on the restricted shells (rbash) the current install command was written to support — the reason it uses `printf | tee` rather than output redirection.
