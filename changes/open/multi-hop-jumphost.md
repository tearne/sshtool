# Multi-hop jumphost chains

**Mode:** _(to be set after Intent approval)_

## Intent

`sshtool` supports exactly one jumphost today — a single `-J host` on the CLI, or a single-hop `ProxyJump` in `~/.ssh/config`; both reject comma-separated multi-hop chains. Real `ssh` allows a chain (`ssh -J a,b,c dest`, or `ProxyJump a,b`), routing through each hop in turn. This change would add chained jumphosts so a connection can traverse several bastions.

This is a connection-layer change, not a config-parsing one: the jumphost is modelled as a single `Option<Endpoint>` and the session keeps a single jump handle alive. Supporting a chain means holding every intermediate session open for the connection's lifetime, with each hop's session riding a channel through the previous one.
