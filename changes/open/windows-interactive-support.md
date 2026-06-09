# Windows interactive terminal support

**Mode:** _(to be set after Intent approval)_

## Intent

The interactive-terminal change is built and verified on Linux, with its Windows runtime behaviour deliberately deferred. This change makes the interactive console work correctly on Windows: confirm and fix raw console mode (character-at-a-time input, virtual-terminal processing for output), and implement window-resize handling — Windows has no `SIGWINCH`, so resize must come another way (polling the console size, or crossterm's resize events).

Until this lands, the Windows binary compiles and runs non-interactive commands, but interactive sessions (`sshtool host` with no command, or `-t`) are unverified there.
