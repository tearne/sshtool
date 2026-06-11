# Startup performance

**Mode:** Wander

## Intent

Investigate slow startup on Windows and reduce binary size where possible.

## Conclusion

Diagnosed 4s startup on Windows as corporate endpoint security inspecting unsigned binaries on every run — nothing addressable from the code side. Added `strip = true` to the release profile (v0.7.7), reducing binary size from 9.8MB to 7.5MB. Remaining delay is a policy matter for IT to whitelist the binary. The earlier planned approaches (aws-lc-rs, current_thread runtime) were not pursued as the bottleneck was external.
