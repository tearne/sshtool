//! Command-line SSH tool: run a command on a remote host (streaming its output)
//! or open an interactive terminal — optionally through a jumphost.

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use sshtool::{Target, interactive, parse_endpoint, run};

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let remote_status = execute(Invocation::parse()).await?;
    Ok(exit_code_from(remote_status))
}

/// No command opens an interactive shell; `-t` runs the command over a PTY (e.g.
/// `tmux attach`); a plain command runs non-interactively and streams its output.
async fn execute(invocation: Invocation) -> Result<u32> {
    let target = target_from(&invocation)?;
    let command = (!invocation.command.is_empty()).then(|| invocation.command.join(" "));
    match command {
        Some(command) if !invocation.force_pty => run(&target, &command).await,
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

    /// Command to run; omit for an interactive shell (a leading `--` is optional)
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}
