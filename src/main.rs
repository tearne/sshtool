//! Command-line SSH tool: run a command on a remote host (streaming its output)
//! or open an interactive terminal — optionally through a jumphost.

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use clap::builder::ArgAction;
use sshtool::{Target, copy_id, interactive, keygen, parse_endpoint, reconnecting_interactive, run, ssh_config, tunnel, tunnel::parse_forward};
#[cfg(windows)]
use sshtool::install;

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let cli = Cli::parse();

    match cli.action {
        None => Ok(exit_code_from(execute(cli.connect).await?)),
        Some(Action::Connect(args)) => Ok(exit_code_from(execute(args).await?)),
        Some(Action::Keygen) => {
            keygen().await?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Action::CopyId { destination, jumphost }) => {
            let target = target_from(&destination, jumphost.as_deref())?;
            copy_id(&target).await?;
            Ok(ExitCode::SUCCESS)
        }
        #[cfg(windows)]
        Some(Action::Install) => {
            install().await?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

async fn execute(args: ConnectArgs) -> Result<u32> {
    let destination = args
        .destination
        .as_deref()
        .context("a destination (user@host or host) is required")?;
    let target = target_from(destination, args.jumphost.as_deref())?;
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

fn target_from(destination_str: &str, jumphost: Option<&str>) -> Result<Target> {
    let destination = parse_endpoint(destination_str)?;
    let config = ssh_config::resolve(&destination.host);

    let username = destination
        .username
        .or_else(|| config.user.clone())
        .context("destination must include a username, as user@host (or set User in ~/.ssh/config)")?;

    let host = config.host_name.clone().unwrap_or(destination.host);
    let port = config.port.unwrap_or(22);

    let jumphost = match jumphost {
        Some(j) => Some(resolve_jumphost(j)?),
        None => resolve_proxy_jump(&config)?,
    };

    Ok(Target { username, host, port, jumphost })
}

/// Turn a `[user@]host` jumphost spec into a resolved endpoint, honouring the
/// same `~/.ssh/config` settings a direct connection would: the alias's
/// `HostName`, and its `User` when the spec names none. Shared by the explicit
/// `-J` flag and a config-derived `ProxyJump` so both resolve identically.
fn resolve_jumphost(spec: &str) -> Result<sshtool::Endpoint> {
    let endpoint = parse_endpoint(spec)?;
    let config = ssh_config::resolve(&endpoint.host);
    Ok(sshtool::Endpoint {
        username: endpoint.username.or(config.user),
        host: config.host_name.unwrap_or(endpoint.host),
    })
}

fn resolve_proxy_jump(config: &ssh_config::HostParams) -> Result<Option<sshtool::Endpoint>> {
    match config.proxy_jump.as_deref() {
        None | Some([]) => Ok(None),
        Some([single]) => Ok(Some(resolve_jumphost(single)?)),
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
    about = "Connect to a remote host over SSH; manage keys with the keygen and copy-id subcommands.",
    subcommand_value_name = "SUBCOMMAND",
    subcommand_help_heading = "Subcommands"
)]
struct Cli {
    #[command(flatten)]
    connect: ConnectArgs,

    #[command(subcommand)]
    action: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Connect to a remote host (the default action when no subcommand is given)
    Connect(ConnectArgs),

    /// Generate an Ed25519 keypair and load it into the SSH agent
    Keygen,

    /// Install the agent's SSH key on the remote host (and jumphost if specified)
    CopyId {
        /// Remote destination as [user@]host
        destination: String,

        /// Jumphost to route through, as [user@]host (SSH-style -J / ProxyJump)
        #[arg(short = 'J', long = "jumphost")]
        jumphost: Option<String>,
    },

    /// Copy the sshtool binary to %USERPROFILE%\bin and add it to the user PATH
    #[cfg(windows)]
    Install,
}

#[derive(Args)]
struct ConnectArgs {
    /// Remote destination as [user@]host
    destination: Option<String>,

    /// Jumphost to route through, as [user@]host (SSH-style -J / ProxyJump)
    #[arg(short = 'J', long = "jumphost")]
    jumphost: Option<String>,

    /// Force a pseudo-terminal: run the command interactively (e.g. `tmux attach`)
    #[arg(short = 't')]
    force_pty: bool,

    /// Do not execute a remote command or shell (forward only)
    #[arg(short = 'N')]
    no_shell: bool,

    /// Reconnect automatically if the connection drops; pair with a remote multiplexer command like `tmux attach` or `zellij attach` to resume the same session
    #[arg(short = 'r', long = "reconnect")]
    reconnect: bool,

    /// Forward a local port to a remote endpoint: local_port:remote_host:remote_port
    #[arg(short = 'L')]
    forwards: Vec<String>,

    /// Verbose output; repeat for more detail (-v connection events, -vv keepalives, -vvv russh internals)
    #[arg(short = 'v', action = ArgAction::Count)]
    verbose: u8,

    /// Command to run; omit for an interactive shell (a leading `--` is optional)
    #[arg(trailing_var_arg = true, value_name = "REMOTE_COMMAND")]
    command: Vec<String>,
}
