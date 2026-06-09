# Forward local stdin to the remote command

**Mode:** _(to be set after Intent approval)_

## Intent

`sshtool` should forward local standard input to the remote command, so data can be piped in — `sshtool host -- wc -l < bigfile`, or `cat local | sshtool host -- 'cat > remote'` — the way `ssh` does. Deferred from the `sshtool` rebaseline, which streams output only.

Likely handled alongside the interactive-console / PTY work, where stdin pumping is already in play, though non-interactive piping is a distinct path that may want its own handling.
