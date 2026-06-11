# Multi-hop key install (CLI)

**Mode:** Formal

## Intent

Add two commands — `keygen` and `copy-id` — that together provision SSH key authentication across a multi-hop path. The target audience (non-experts on Windows) find `ssh-keygen` + `ssh-copy-id` too fiddly, especially when a jumphost is involved. `keygen` gets the agent into a known good state (exactly one key loaded); `copy-id` installs whatever the agent holds onto every hop. No TUI required; the TUI will later surface these as a wizard.

## Approach

### Two commands, not one

`keygen` and `copy-id` are separate commands with distinct responsibilities. `keygen` owns the local key lifecycle (generate, load into agent). `copy-id` owns remote installation. The agent is the handoff point between them — `copy-id` never touches disk, only the agent.

### `keygen` flow

The agent is the source of truth. Checked first:

1. **Agent has exactly one key** — report it, suggest `copy-id`. Stop.
2. **Agent has multiple keys** — report count, refuse, tell user to run `ssh-add -D` then retry. Stop.
3. **Agent empty, one key on disk** — report it, prompt "Load into agent? [Y/n]", prompt passphrase if needed, load. Suggest `copy-id`.
4. **Agent empty, multiple keys on disk** — report them, refuse, tell user to clean up manually. Stop.
5. **Agent empty, no key on disk** — prompt passphrase (twice, matching `ssh-keygen`), generate Ed25519 keypair, write to `~/.ssh/id_ed25519` + `.pub`, load into agent. Suggest `copy-id`.

No `-f` flag. Any scenario requiring a force is one where the user should act deliberately; the refusal message tells them exactly what to do.

### `copy-id` flow

1. **Agent has exactly one key** — install its public key on each hop (jumphost first if present, then destination). Verify key auth on each hop after install.
2. **Agent has multiple keys** — refuse, suggest cleanup.
3. **Agent has no keys** — refuse, suggest `sshtool keygen`.

Installation command on each hop: `mkdir -p ~/.ssh && chmod 700 ~/.ssh && echo '<pubkey>' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys`. The existing `connection` and `authentication` machinery handles password/agent auth to reach each hop.

### Module restructuring

Rename `src/setup.rs` → `src/key_install.rs`. Split into `keygen()` and `copy_id()` functions. The `setup` subcommand in `main.rs` is replaced by `keygen` and `copy-id` subcommands (both pre-routed via the `args[1]` check already in place).

### Passphrase handling

Prompt using `rpassword` (already a dependency). For generation: prompt twice and confirm match, empty passphrase accepted (stored unencrypted). For loading an existing key: prompt once, pass to `ssh-add` equivalent via russh agent API.

## Plan

- [x] Rename `src/setup.rs` → `src/key_install.rs`; update `lib.rs` and `main.rs` references
- [x] Implement `keygen()` in `src/key_install.rs` with the five-case agent-first flow
- [x] Implement `copy_id()` in `src/key_install.rs` with the three-case agent flow
- [x] Wire `keygen` and `copy-id` subcommands in `main.rs` (replacing `setup`)
- [x] Bump version to 0.8.1

## Log

- Initial build shipped as `setup` (0.8.0). Design rethought: split into `keygen` + `copy-id` with agent-first flow, passphrase support, and strict single-key enforcement. No `-f` flag by design.
- clap subcommand approach conflicted with the existing flat `SshArgs` parser; resolved by pre-routing via manual `args[1]` check.
- `authenticate_publickey` requires `PrivateKeyWithHashAlg`, not `PrivateKey` directly.

## Conclusion

Delivered `sshtool keygen` and `sshtool copy-id` — a two-step key provisioning flow for non-expert users. `keygen` checks the agent first, discovers existing keys by scanning `~/.ssh/` for OpenSSH-format files (not just standard names), handles passphrase generation correctly without re-prompting, and loads directly from memory. `copy-id` installs via `printf | tee -a` for rbash compatibility, shares a password store across hops, and verifies via a fresh connection rather than re-authenticating on an existing session.
