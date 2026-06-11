# Startup performance

**Mode:** Explore

## Intent

The application starts noticeably slowly. Investigate the root causes — likely candidates are the `ring` crypto backend (C-based, heavyweight initialisation), the tokio multi-threaded runtime (thread pool spin-up on a tool that does sequential work), and possibly the `flate2` compression feature — measure to confirm, and apply whichever fixes prove worthwhile.

## Approach

### Switch russh from `ring` to `aws-lc-rs`

The current `Cargo.toml` overrides russh's default by explicitly requesting `ring`. The russh default is `aws-lc-rs`, which has lighter initialisation overhead and is the better cross-platform choice (full Windows support, no system library dependency). The fix is to remove `ring` from the feature list and remove `flate2` while there — it is also in our explicit list and unnecessary on fast networks. No code changes required; only `Cargo.toml` changes.

### Switch tokio to `current_thread` runtime

`#[tokio::main]` defaults to the multi-threaded scheduler, which spins up a thread pool on every invocation. The tool does sequential SSH work with no CPU-bound parallelism, so the single-threaded `current_thread` runtime is a straightforward fit. Change is a one-line annotation on `main`.

## Topics

- Drop `ring` and `flate2` from russh features in `Cargo.toml`, confirm `aws-lc-rs` is active
- Switch `#[tokio::main]` to `#[tokio::main(flavor = "current_thread")]` in `main.rs`

**Done when:** both changes build cleanly on the current platform and a manual timing check shows a subjective improvement in startup speed.
