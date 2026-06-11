//! Command-line SSH tool: run a command on a remote host (streaming its output)
//! or open an interactive terminal — optionally through a jumphost.

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use clap::builder::ArgAction;
use sshtool::{Target, copy_id, interactive, keygen, parse_endpoint, reconnecting_interactive, run, tunnel, tunnel::parse_forward};

#[tokio::main]
async fn main() -> Result<ExitCode> {
    // Route utility subcommands before clap sees them, to avoid conflicting parsers.
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("keygen") => {
            keygen().await?;
            return Ok(ExitCode::SUCCESS);
        }
        Some("copy-id") => {
            let copy_id_args = CopyIdArgs::parse_from(&args[1..]);
            let target = target_from_copy_id(&copy_id_args)?;
            copy_id(&target).await?;
            return Ok(ExitCode::SUCCESS);
        }
        _ => {}
    }

    let invocation = SshArgs::parse();
    let remote_status = execute(invocation).await?;
    Ok(exit_code_from(remote_status))
}

async fn execute(invocation: SshArgs) -> Result<u32> {
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

fn target_from(invocation: &SshArgs) -> Result<Target> {
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

fn target_from_copy_id(args: &CopyIdArgs) -> Result<Target> {
    let destination = parse_endpoint(&args.destination)?;
    let username = destination
        .username
        .context("destination must include a username, as user@host")?;
    let jumphost = args
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

fn exit_code_from(remote_status: u32) -> ExitCode {
    ExitCode::from(remote_status as u8)
}

/// `sshtool [-t] user@host [-J [user@]host] [--] [command...]`
/// `sshtool keygen`
/// `sshtool copy-id user@host [-J [user@]host]`
#[derive(Parser)]
#[command(
    version,
    about = "Run a command on a remote host through SSH, or open an interactive terminal.\nUse `sshtool keygen` to generate a key, then `sshtool copy-id user@host` to install it."
)]
struct SshArgs {
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

/// `sshtool copy-id user@host [-J jumphost]`
#[derive(Parser)]
#[command(about = "Install the agent's SSH key on the remote host (and jumphost if specified)")]
struct CopyIdArgs {
    /// Remote destination as user@host
    destination: String,

    /// Jumphost to route through, as [user@]host (SSH-style -J / ProxyJump)
    #[arg(short = 'J', long = "jumphost")]
    jumphost: Option<String>,
}
