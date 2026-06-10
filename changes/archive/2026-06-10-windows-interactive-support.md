# Windows interactive terminal support

**Mode:** Explore

## Intent

The interactive-terminal change is built and verified on Linux, with its Windows runtime behaviour deliberately deferred. This change makes the interactive console work correctly on Windows: confirm and fix raw console mode (character-at-a-time input, virtual-terminal processing for output), and implement window-resize handling — Windows has no `SIGWINCH`, so resize must come another way (polling the console size, or crossterm's resize events).

Until this lands, the Windows binary compiles and runs non-interactive commands, but interactive sessions (`sshtool host` with no command, or `-t`) are unverified there.

## Approach

### Windows raw input as a VT byte stream

Enable `ENABLE_VIRTUAL_TERMINAL_INPUT` on the console input handle so keystrokes — arrows and function keys included — arrive as a VT escape-sequence byte stream that the existing threaded stdin reader forwards unchanged. This keeps the parent change's raw-passthrough principle (no re-encoding of crossterm key events). crossterm's `enable_raw_mode` clears echo/line/processed input on Windows but does not set VT input, so we set it ourselves.

### Windows VT output processing

Enable `ENABLE_VIRTUAL_TERMINAL_PROCESSING` on the console output handle so the remote's ANSI escapes render rather than printing literally. We write raw bytes via tokio stdout, bypassing the crossterm output path that would otherwise enable this, so we enable it explicitly.

### Console-mode toggles live in the raw-mode guard

Extend `RawMode` (Windows only) to capture the original input and output console modes on entry and restore them on drop, alongside crossterm's raw-mode toggle — so the VT flags are undone on exit, error, or panic exactly as raw mode already is.

### Resize by polling the console size

Replace the Windows `ResizeWatcher` no-op with a poller that samples `terminal::size()` every 200 ms and fires `window_change` when it changes. Windows has no `SIGWINCH`, and crossterm's resize event shares the console-input handle we read raw bytes from via std stdin — consuming both would conflict — so polling is the non-conflicting source.

### Console modes set via `windows-sys`

Add `windows-sys` as a Windows-only target dependency for `GetConsoleMode`/`SetConsoleMode`; crossterm exposes no public toggle for the VT input flag, and `windows-sys` is already in the tree transitively via crossterm.

### Platform scope: code here, runtime verified on Windows

Build and cross-build succeed in this environment; the live behaviour — raw mode, VT rendering, resize reflow — is verified by the user on a real Windows terminal, since there is no Windows host here. This mirrors the parent change, whose Windows runtime was deferred for the same reason.

The decisive input check is arrow keys: at the remote shell, `cat -v` should print `^[[A`/`^[[B`/`^[[C`/`^[[D` and editors should navigate. Plain letters echoing while arrows are dead or garbled means `std::io::stdin().read()` isn't delivering the VT byte stream despite the flag — the only real runtime risk, fixed if it surfaces by reading the console through a console-specific path (`ReadConsoleInput`) rather than std stdin. Not built speculatively. Output is independent: if remote colours and full-screen redraws render, output VT is fine regardless.

## Plan

**Topics**

- **Console VT modes** — on Windows, enable `ENABLE_VIRTUAL_TERMINAL_INPUT` on the input handle and `ENABLE_VIRTUAL_TERMINAL_PROCESSING` on the output handle, capturing the prior modes so they can be restored.

- **Raw-mode guard restoration** — fold the VT-mode capture and restore into `RawMode` so exit, error, and panic all undo the flags alongside crossterm's raw toggle.

- **Windows resize poller** — replace the `#[cfg(not(unix))]` `ResizeWatcher` no-op with a 200 ms `terminal::size()` poll that signals `changed()` only when the size differs from the last sample.

- **`windows-sys` dependency** — declare it as a Windows-target dependency with the `Win32_System_Console` feature for the console-mode calls.

**Done when** — `cargo build --target x86_64-pc-windows-gnu` still succeeds, the Unix path is unchanged (build + clippy clean, tests pass), and on a real Windows terminal `sshtool host` gives a usable raw shell (arrow keys and editors work per the `cat -v` check), remote ANSI renders, and resizing the window reflows full-screen apps.

## Log

- `windows-sys` 0.59 was already in the dependency tree transitively, so declaring it direct pulled no new copy. Used `Win32_System_Console` plus `Win32_Foundation` (for `INVALID_HANDLE_VALUE`).
- tokio's `time` feature (needed by the resize poller's `interval`) added as a `cfg(not(unix))` target dependency rather than to the shared feature list, keeping the Linux build lean — the poller is Unix-excluded anyway.
- Restore order matters: the `RawMode` drop restores our VT flags *before* `disable_raw_mode`, because crossterm only saves/restores the input handle. Our restore puts output VT back (crossterm never touched output) and re-applies the captured input mode, then crossterm's restore of the true original input mode has the final say.
- Verified here: Linux build + clippy + 6 tests clean; Windows cross-build (`x86_64-pc-windows-gnu`) compiles the real `#[cfg(windows)]` paths and the poller, so the `windows-sys` API usage type-checks; clippy clean on the Windows target too. Live Windows runtime (raw input via std stdin under VT mode, ANSI rendering, resize reflow) is unverifiable here — rests on the user, with the `cat -v` arrow-key check as the first signal for the input risk.
- **Parked here, awaiting user's Windows testing.** All Plan topics built and machine-verified; the change stays active because code is changed. **To resume:** the user tests on a real Windows terminal — (1) `cat -v` then arrow keys should print `^[[A`/`^[[B`/`^[[C`/`^[[D` and editors should navigate; (2) remote colours / full-screen redraws render (output VT); (3) resizing the window reflows full-screen apps (the 200 ms poller). If letters echo but arrows are dead/garbled, the std-stdin-under-VT input risk surfaced — fall back to reading the console via `ReadConsoleInput`. On a clean pass, draft the Conclusion + a changelog entry (currently version 0.6.0) and archive.

## Conclusion

All Plan topics built and machine-verified (Linux build + clippy + 6 tests clean; Windows cross-build compiles and type-checks the `#[cfg(windows)]` paths). **Accepted without live Windows testing at the user's explicit decision** — the three runtime checks (raw VT arrow-key input, ANSI output rendering, resize reflow) were not exercised on a real Windows terminal. The identified residual risk remains: if std-stdin under `ENABLE_VIRTUAL_TERMINAL_INPUT` doesn't deliver the VT byte stream, arrow keys will be dead/garbled, and the fallback is to read the console via `ReadConsoleInput`. Tracked here rather than left dangling.

No deviations from the Approach. Version bumped to 0.6.0 in `Cargo.toml` during build.
