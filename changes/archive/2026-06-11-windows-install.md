# Windows install command

**Mode:** Formal

## Intent

Add a `sshtool install` subcommand (Windows-only) that installs the binary into `%USERPROFILE%\bin`, adds that directory to the user PATH if not already present, and tells the user to open a new terminal. No admin rights required. Lets users go from a downloaded `.exe` to a properly installed tool with one command. Invoked as `./sshtool install` on first use.

## Approach

### CLI routing

`install` is routed via the existing `args[1]` match in `main.rs`, consistent with `keygen` and `copy-id`.

### Windows-only

The `install` subcommand is compiled only on Windows (`#[cfg(windows)]`). On other platforms the match arm is absent; if somehow invoked it falls through to the normal SSH argument parser and produces a "destination required" error — acceptable since this is a Windows-specific feature.

### Install steps

1. Resolve the running executable's path via `std::env::current_exe()`.
2. Create `%USERPROFILE%\bin` if it doesn't exist.
3. Copy the executable there as `sshtool.exe`.
4. Read the user PATH from the registry (`HKCU\Environment\PATH`), check if `%USERPROFILE%\bin` is already present, and append it if not — using the Windows registry API via `windows-sys`.
5. Print clear confirmation and tell the user to open a new terminal.

### Registry vs `SetEnvironmentVariable`

Writing directly to `HKCU\Environment\PATH` in the registry is the correct persistent approach — it's what `SetEnvironmentVariable("PATH", ..., "User")` does under the hood, and it survives reboots and new terminal sessions. The `windows-sys` crate already in the dependency tree provides `RegOpenKeyExW`, `RegQueryValueExW`, and `RegSetValueExW`.

### New module: `install`

Logic lives in `src/install.rs`, gated `#[cfg(windows)]`. Exposed from `lib.rs` as `pub async fn install() -> Result<()>`.

## Plan

- [ ] Create `src/install.rs` (Windows-only): resolve current exe, create `%USERPROFILE%\bin`, copy binary, update user PATH in registry if needed
- [ ] Expose `install` from `lib.rs` behind `#[cfg(windows)]`
- [ ] Wire `install` subcommand in `main.rs` behind `#[cfg(windows)]`
- [x] Bump version

## Log

- Windows build failed silently on first attempt — `install.rs` used `isize` for `HKEY` but the API requires `*mut c_void`. Fixed after inspecting the Windows build errors directly.
- Second run from the installed location errored — process can't overwrite itself on Windows. Fixed by detecting same-file via `canonicalize` and skipping the copy.

## Conclusion

`sshtool install` copies the binary to `%USERPROFILE%\bin` and adds it to the user PATH via the registry. Idempotent — re-running from the installed location skips the copy. PATH changes take effect in new terminal sessions, as expected.
