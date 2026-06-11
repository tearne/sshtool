# Multi-hop key install (CLI)

**Mode:** _(to be set after Intent approval)_

## Intent

Add a setup subcommand that provisions an SSH keypair onto both a jumphost and a destination host in one operation — the common case for the target audience, who reach destinations through a jumphost and find manual `ssh-copy-id` across multiple hops too fiddly. Reuses the existing auth and jumphost machinery. No TUI required; this is a CLI-first flow that the TUI front door will later surface as a wizard.
