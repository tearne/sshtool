# LocalForward from ssh config

**Mode:** Formal

## Intent

When `~/.ssh/config` defines `LocalForward` entries for a host, `sshtool` should honour them automatically on connection — the same way it already honours `HostName`, `User`, and `ProxyJump`. Forwards from the config are merged with any `-L` flags given on the command line. The combined set is active for all connection paths (interactive, command, and explicit tunnel).
