# sshtool

Reach a remote host over SSH — optionally through a jumphost — and either run a command (streaming its output) or open an interactive terminal, the way `ssh` does. A small Rust library plus a command-line tool.

## What it does

Given a destination and optionally a jumphost, `sshtool`:

1. connects to the remote host (directly, or by tunnelling a second SSH session through the jumphost),
2. authenticates each hop — using a key from a running ssh-agent if one is available, otherwise prompting for a password,
3. either runs a command (streaming its stdout/stderr and exiting with its status) or opens an interactive shell over a pseudo-terminal.

Each hop authenticates with its own username: the destination's comes from `user@host`, and the jumphost's from its own `user@host` — or the destination's username if the jumphost is given as a bare host.

## Using the command-line tool

```
sshtool [-t] [-A] user@host [-J [user@]host] [--] [command...]
```

With a command, it runs and streams the output. With **no command**, it opens an **interactive shell**. With **`-t`**, it runs the command interactively over a PTY (for full-screen programs like `tmux`). `-J` is the SSH-style jumphost flag (`--jumphost` is accepted too). **`-A`** forwards your local ssh-agent to the destination — never to a jumphost — so tools there (`git`, a further `ssh`) can use your keys without them leaving your machine; `ForwardAgent yes` for the host in `~/.ssh/config` does the same without the flag. A leading `--` before the command is optional. Examples:

```sh
# Run a command and stream its output
sshtool alice@remote.example cat /etc/hostname

# Open an interactive shell
sshtool alice@remote.internal

# Drop straight into a tmux session (interactive, over a PTY)
sshtool -t alice@remote.internal tmux attach

# Reconnect and attach to the `main` Zellij session, creating it if it doesn't exist (shorthand for -r host -- "zellij attach -c main")
sshtool -z alice@remote.internal

# Use your local keys on the remote, e.g. to pull from a private git repo
sshtool -A alice@remote.internal git -C project pull

# Through a jumphost (its own username, or the destination's if omitted)
sshtool alice@remote.internal -J bob@gateway.example tail -n 50 /var/log/report.txt
```

Command output goes to stdout, so redirect or pipe it — including onto the clipboard with the OS tool: `sshtool host cat file | clip` on Windows, `| pbcopy` on macOS, `| xclip` on Linux.

## Using the library

```rust
use sshtool::{interactive, parse_endpoint, run, Target};

let target = Target {
    username: "alice".into(),
    host: "remote.internal".into(),
    jumphost: Some(parse_endpoint("bob@gateway.example")?), // or "gateway.example" to reuse "alice"
};

// Run a command, streaming its output; returns the remote exit status.
let exit_status: u32 = run(&target, "cat /etc/hostname").await?;

// Or open an interactive terminal: a shell (None) or a command over a PTY.
let exit_status: u32 = interactive(&target, None).await?;
```

## Building

Requires the [Rust toolchain](https://rustup.rs). There are no OpenSSL or libssh2 dependencies, but the SSH crypto backend (`ring`) compiles a small amount of C and assembly, so a C compiler for the target you are building is needed. Building natively for the host you are already on, that compiler is usually present.

```sh
cargo build --release
```

The binary lands at `target/release/sshtool` (`sshtool.exe` on Windows).

### Building the Windows binary

Build natively on a Windows machine:

1. Install Rust from <https://rustup.rs>. The default toolchain is `x86_64-pc-windows-msvc`, whose linker needs the **Visual Studio C++ Build Tools** (the "Desktop development with C++" workload). The rustup installer prompts for this and can guide you through it.
2. From the project directory:
   ```sh
   cargo build --release
   ```
3. The executable is `target\release\sshtool.exe`. Copy it wherever you like and run it from a terminal:
   ```
   sshtool.exe alice@remote.internal -J gateway.example -- cat C:\path\to\file.txt
   ```

### Cross-compiling the Windows binary from Linux

You can produce `sshtool.exe` on a Linux machine without Windows — you just can't run it there. The only system dependency is the **mingw-w64** cross-toolchain, which supplies the linker and the `dlltool`/`gcc` that the build needs (the `ring` crypto backend has no cmake or nasm requirement).

1. Install the cross-toolchain:
   ```sh
   sudo apt install mingw-w64          # Debian / Ubuntu
   ```
2. Add the Rust standard library for the target:
   ```sh
   rustup target add x86_64-pc-windows-gnu
   ```
3. Build:
   ```sh
   cargo build --release --target x86_64-pc-windows-gnu
   ```

The executable is `target/x86_64-pc-windows-gnu/release/sshtool.exe`.

The resulting `sshtool.exe` is self-contained: it imports only standard Windows system DLLs (no mingw runtime DLLs, no Visual C++ Redistributable). Copy the single `.exe` to the target machine and run it — no installation and no administrator rights are needed.

