# Support for ~/.ssh/config

**Mode:** _(to be set after Intent approval)_

## Intent

`sshtool` should read connection settings from the user's OpenSSH client config (`~/.ssh/config`) so that an invocation like `sshtool myalias -- …` honours what the user already has set up for `ssh` — host aliases and their `HostName`, `User`, `Port`, `ProxyJump`, `IdentityFile`, and similar — instead of forcing every value onto the command line. This matches user expectations and reduces flags, especially as jumphosts and forwarding grow.

Scope (which directives to honour, precedence against explicit flags) to be worked out when planned.
