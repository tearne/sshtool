//! Command-line SSH tool: run a command on a remote host (streaming its output)
//! or open an interactive terminal — optionally through a jumphost.

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use clap::builder::ArgAction;
use sshtool::{Target, interactive, parse_endpoint, reconnecting_interactive, run, tunnel, tunnel::parse_forward};

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let remote_status = execute(Invocation::parse()).await?;
    Ok(exit_code_from(remote_status))
}

/// No command opens an interactive shell; `-t` runs the command over a PTY (e.g.
/// `tmux attach`); a plain command runs non-interactively and streams its output.
/// `-L` adds local port forwards (auto-reconnecting); `-N` suppresses the shell.
async fn execute(invocation: Invocation) -> Result<u32> {
    let target = target_from(&invocation)?;
    let command = (!invocation.command.is_empty()).then(|| invocation.command.join(" "));

    if invocation.no_shell && command.is_some() {
        anyhow::bail!("-N (no shell) and a command are mutually exclusive");
    }

    if !invocation.forwards.is_empty() {
        let forwards = invocation
            .forwards
            .iter()
            .map(|s| parse_forward(s))
            .collect::<Result<Vec<_>>>()?;
        tunnel(&target, &forwards, invocation.verbose).await?;
        return Ok(0);
    }

    match command {
        Some(command) if !invocation.force_pty && !invocation.reconnect => {
            run(&target, &command).await
        }
        command if invocation.reconnect => {
            reconnecting_interactive(&target, command.as_deref(), invocation.verbose).await
        }
        command => interactive(&target, command.as_deref()).await,
    }
}

fn target_from(invocation: &Invocation) -> Result<Target> {
    let destination = parse_endpoint(&invocation.destination)?;
    let username = destination
        .username
        .context("destination must include a username, as user@host")?;
    let jumphost = invocation
        .jumphost
        .as_deref()
        .map(parse_endpoint)
        .transpose()?;
    Ok(Target {
        username,
        host: destination.host,
        jumphost,
    })
}

/// Mirror `ssh`: exit with the remote command's status. A process exit code
/// carries only a byte, so anything wider is truncated to it.
fn exit_code_from(remote_status: u32) -> ExitCode {
    ExitCode::from(remote_status as u8)
}

/// `sshtool [-t] user@host [-J [user@]host] [--] [command...]`
#[derive(Parser)]
#[command(
    version,
    about = "Run a command on a remote host through SSH, or open an interactive terminal"
)]
struct Invocation {
    /// Remote destination as user@host
    destination: String,

    /// Jumphost to route through, as [user@]host (SSH-style -J / ProxyJump).
    /// Uses its own username if given, otherwise the destination's.
    #[arg(short = 'J', long = "jumphost")]
    jumphost: Option<String>,

    /// Force a pseudo-terminal: run the command interactively (e.g. `tmux attach`)
    #[arg(short = 't')]
    force_pty: bool,

    /// Do not execute a remote command or shell (forward only)
    #[arg(short = 'N')]
    no_shell: bool,

    /// Reconnect automatically if the connection drops (for use with remote multiplexers like zellij/tmux)
    #[arg(short = 'r', long = "reconnect")]
    reconnect: bool,

    /// Forward a local port to a remote endpoint: local_port:remote_host:remote_port
    #[arg(short = 'L')]
    forwards: Vec<String>,

    /// Verbose output; repeat for more detail (-v connection events, -vv keepalives, -vvv russh internals)
    #[arg(short = 'v', action = ArgAction::Count)]
    verbose: u8,

    /// Command to run; omit for an interactive shell (a leading `--` is optional)
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}
