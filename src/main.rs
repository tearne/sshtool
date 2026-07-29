//! Command-line SSH tool: run a command on a remote host (streaming its output)
//! or open an interactive terminal — optionally through a jumphost.

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use clap::builder::ArgAction;
use sshtool::{Target, copy_id, interactive, keygen, parse_endpoint, reconnecting_interactive, run, ssh_config, tunnel, tunnel::parse_forward};
#[cfg(windows)]
use sshtool::install;

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let args = Args::parse();

    if args.keygen {
        keygen().await?;
        return Ok(ExitCode::SUCCESS);
    }

    #[cfg(windows)]
    if args.install {
        install().await?;
        return Ok(ExitCode::SUCCESS);
    }

    if args.copy_id {
        let target = target_from(&args)?;
        copy_id(&target).await?;
        return Ok(ExitCode::SUCCESS);
    }

    let remote_status = execute(args).await?;
    Ok(exit_code_from(remote_status))
}

async fn execute(args: Args) -> Result<u32> {
    let target = target_from(&args)?;
    let command = (!args.command.is_empty()).then(|| args.command.join(" "));

    if args.no_shell && command.is_some() {
        anyhow::bail!("-N (no shell) and a command are mutually exclusive");
    }

    if !args.forwards.is_empty() {
        let forwards = args
            .forwards
            .iter()
            .map(|s| parse_forward(s))
            .collect::<Result<Vec<_>>>()?;
        tunnel(&target, &forwards, args.verbose).await?;
        return Ok(0);
    }

    match command {
        Some(command) if !args.force_pty && !args.reconnect => {
            run(&target, &command).await
        }
        command if args.reconnect => {
            reconnecting_interactive(&target, command.as_deref(), args.verbose).await
        }
        command => interactive(&target, command.as_deref()).await,
    }
}

fn target_from(args: &Args) -> Result<Target> {
    let destination_str = args.destination.as_deref()
        .context("a destination (user@host or host) is required")?;
    let destination = parse_endpoint(destination_str)?;
    let config = ssh_config::resolve(&destination.host);

    let username = destination
        .username
        .or_else(|| config.user.clone())
        .context("destination must include a username, as user@host (or set User in ~/.ssh/config)")?;

    let host = config.host_name.clone().unwrap_or(destination.host);
    let port = config.port.unwrap_or(22);

    let jumphost = if let Some(ref j) = args.jumphost {
        Some(parse_endpoint(j)?)
    } else {
        resolve_proxy_jump(&config)?
    };

    Ok(Target { username, host, port, jumphost })
}

fn resolve_proxy_jump(config: &ssh_config::HostParams) -> Result<Option<sshtool::Endpoint>> {
    match config.proxy_jump.as_deref() {
        None | Some([]) => Ok(None),
        Some([single]) => {
            let endpoint = parse_endpoint(single)?;
            let jump_config = ssh_config::resolve(&endpoint.host);
            Ok(Some(sshtool::Endpoint {
                username: endpoint.username,
                host: jump_config.host_name.unwrap_or(endpoint.host),
            }))
        }
        Some(_) => anyhow::bail!(
            "~/.ssh/config specifies multiple ProxyJump hops for this host, \
             which sshtool does not currently support. Use a single hop or pass -J explicitly."
        ),
    }
}

fn exit_code_from(remote_status: u32) -> ExitCode {
    ExitCode::from(remote_status as u8)
}

#[derive(Parser)]
#[command(
    version,
    about = "Connect to a remote host over SSH, or use --keygen / --copy-id to manage keys."
)]
struct Args {
    /// Remote destination as [user@]host
    #[cfg_attr(windows, arg(conflicts_with_all = ["keygen", "install"]))]
    #[cfg_attr(not(windows), arg(conflicts_with_all = ["keygen"]))]
    destination: Option<String>,

    /// Generate an Ed25519 keypair and load it into the SSH agent
    #[cfg_attr(windows, arg(long, conflicts_with_all = ["install", "copy_id", "jumphost", "force_pty", "no_shell", "reconnect", "forwards", "verbose"]))]
    #[cfg_attr(not(windows), arg(long, conflicts_with_all = ["copy_id", "jumphost", "force_pty", "no_shell", "reconnect", "forwards", "verbose"]))]
    keygen: bool,

    /// Install the agent's SSH key on the remote host (and jumphost if specified)
    #[arg(long, conflicts_with_all = ["keygen", "force_pty", "no_shell", "reconnect", "forwards", "verbose"])]
    copy_id: bool,

    /// Copy the sshtool binary to %USERPROFILE%\bin and add it to the user PATH
    #[cfg(windows)]
    #[arg(long, conflicts_with_all = ["keygen", "copy_id", "jumphost", "force_pty", "no_shell", "reconnect", "forwards", "verbose"])]
    install: bool,

    /// Jumphost to route through, as [user@]host (SSH-style -J / ProxyJump)
    #[arg(short = 'J', long = "jumphost", conflicts_with = "keygen")]
    jumphost: Option<String>,

    /// Force a pseudo-terminal: run the command interactively (e.g. `tmux attach`)
    #[arg(short = 't', conflicts_with_all = ["keygen", "copy_id"])]
    force_pty: bool,

    /// Do not execute a remote command or shell (forward only)
    #[arg(short = 'N', conflicts_with_all = ["keygen", "copy_id"])]
    no_shell: bool,

    /// Reconnect automatically if the connection drops (for use with remote multiplexers like zellij/tmux)
    #[arg(short = 'r', long = "reconnect", conflicts_with_all = ["keygen", "copy_id"])]
    reconnect: bool,

    /// Forward a local port to a remote endpoint: local_port:remote_host:remote_port
    #[arg(short = 'L', conflicts_with_all = ["keygen", "copy_id"])]
    forwards: Vec<String>,

    /// Verbose output; repeat for more detail (-v connection events, -vv keepalives, -vvv russh internals)
    #[arg(short = 'v', action = ArgAction::Count, conflicts_with_all = ["keygen", "copy_id"])]
    verbose: u8,

    /// Command to run; omit for an interactive shell (a leading `--` is optional)
    #[arg(trailing_var_arg = true, conflicts_with_all = ["keygen", "copy_id"])]
    command: Vec<String>,
}
