# Resolve explicit -J jumphost through ssh config

**Mode:** _(to be set after Intent approval)_

## Intent

A jumphost passed explicitly with `-J <alias>` is used literally — `sshtool` connects to the raw string rather than resolving it through `~/.ssh/config`. So `sshtool -J jh2 ranch-1127 -- pwd` fails with "No such host is known" because `jh2` is a config alias whose real `HostName` (`ssh2.int.sce.network`) is never looked up. Meanwhile the same alias works when it comes from a host's `ProxyJump` in config, because that path already resolves `HostName`. `ssh -J jh2 …` resolves the alias; `sshtool` should match it.

An explicit `-J` alias should honour the same `~/.ssh/config` settings a config-derived `ProxyJump` does — at least `HostName` and `User` — so the two ways of specifying a jumphost behave consistently.
