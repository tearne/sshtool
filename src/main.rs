//! Command-line SSH tool: run a command on a remote host (streaming its output)
//! or open an interactive terminal — optionally through a jumphost.

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use clap::builder::ArgAction;
use sshtool::{Foreground, Target, copy_id, keygen, parse_endpoint, run, ssh_config, tunnel::parse_forward};
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

async fn execute(mut args: ConnectArgs) -> Result<u32> {
    expand_zellij_shorthand(&mut args)?;

    let destination = args
        .destination
        .as_deref()
        .context("a destination (user@host or host) is required")?;
    let target = target_from(destination, args.jumphost.as_deref())?;
    let command = (!args.command.is_empty()).then(|| args.command.join(" "));

    if args.no_shell && command.is_some() {
        anyhow::bail!("-N (no shell) and a command are mutually exclusive");
    }

    let forwards = forwards_for(destination, &args.forwards)?;
    let foreground = foreground_for(&args, command);
    let forward_agent = forward_agent_for(destination, args.forward_agent)?;

    run(&target, forwards, foreground, args.reconnect, forward_agent, args.verbose).await
}

/// `-z` is shorthand for reconnecting and attaching to the `main` Zellij
/// session, creating it if it doesn't exist. It supplies the remote command
/// itself, so it can't sit alongside an explicit command or `-N`. An explicit
/// `-r` is redundant, not a conflict.
fn expand_zellij_shorthand(args: &mut ConnectArgs) -> Result<()> {
    if !args.zellij {
        return Ok(());
    }
    if !args.command.is_empty() {
        anyhow::bail!("-z and an explicit command are mutually exclusive");
    }
    if args.no_shell {
        anyhow::bail!("-z and -N (no shell) are mutually exclusive");
    }
    args.reconnect = true;
    args.command = vec!["zellij attach -c main".to_string()];
    Ok(())
}

/// Merge the host's config `LocalForward` entries with any `-L` flags into one
/// forward set, config first, and parse each into a bindable forward. Both
/// sources already share the `[bind:]port:host:hostport` colon form.
fn forwards_for(
    destination: &str,
    cli_forwards: &[String],
) -> Result<Vec<sshtool::tunnel::LocalForward>> {
    let endpoint = parse_endpoint(destination)?;
    let config = ssh_config::resolve(&endpoint.host);
    config
        .local_forwards
        .iter()
        .chain(cli_forwards)
        .map(|spec| parse_forward(spec))
        .collect()
}

/// `-A` turns agent forwarding on; without it the host's `ForwardAgent` decides,
/// defaulting to off.
fn forward_agent_for(destination: &str, cli_forward_agent: bool) -> Result<bool> {
    if cli_forward_agent {
        return Ok(true);
    }
    let endpoint = parse_endpoint(destination)?;
    Ok(ssh_config::resolve(&endpoint.host).forward_agent.unwrap_or(false))
}

/// Decide what runs in the foreground: `-N` suppresses it entirely; a bare
/// command with neither `-t` nor `-r` streams without a PTY; everything else —
/// a shell, a forced-PTY command, or a reconnecting one — runs interactively.
/// A command over a PTY is run through a login shell so it inherits the user's
/// profile `PATH`, matching what an interactive shell would find.
fn foreground_for(args: &ConnectArgs, command: Option<String>) -> Foreground {
    if args.no_shell {
        Foreground::None
    } else if let Some(command) = command {
        if !args.force_pty && !args.reconnect {
            Foreground::Command(command)
        } else {
            Foreground::Interactive(Some(login_shell_wrap(&command)))
        }
    } else {
        Foreground::Interactive(None)
    }
}

/// Wrap a command so a login shell runs it, sourcing the user's profile (where
/// `PATH` entries like `~/.local/bin` are built). `$SHELL` honours the account's
/// own shell, falling back to `bash` when unset. The command is single-quoted —
/// each embedded quote closed, escaped, and reopened — so its metacharacters
/// reach the login shell verbatim.
fn login_shell_wrap(command: &str) -> String {
    let escaped = command.replace('\'', "'\\''");
    format!("${{SHELL:-bash}} -lc '{escaped}'")
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

    /// Force a pseudo-terminal and run the command through a login shell, so it sees your profile PATH (e.g. `tmux attach`, `zellij a`)
    #[arg(short = 't')]
    force_pty: bool,

    /// Do not execute a remote command or shell (forward only)
    #[arg(short = 'N')]
    no_shell: bool,

    /// Reconnect automatically if connection drops, e.g. pair with a remote multiplexer command like `zellij a` to resume sessions. Implies -t
    #[arg(short = 'r', long = "reconnect")]
    reconnect: bool,

    /// Shorthand for `-r host -- "zellij attach -c main"`: reconnect and attach to the `main` Zellij session, creating it if it doesn't exist
    #[arg(short = 'z')]
    zellij: bool,

    /// Forward the local ssh-agent to the destination, so remote tools can use your keys (as `ForwardAgent yes` in ~/.ssh/config)
    #[arg(short = 'A')]
    forward_agent: bool,

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

#[cfg(test)]
mod tests {
    use super::login_shell_wrap;

    #[test]
    fn wraps_a_plain_command_in_a_login_shell() {
        assert_eq!(login_shell_wrap("zellij a"), "${SHELL:-bash} -lc 'zellij a'");
    }

    #[test]
    fn escapes_embedded_single_quotes() {
        assert_eq!(
            login_shell_wrap("echo 'hi'"),
            r#"${SHELL:-bash} -lc 'echo '\''hi'\'''"#
        );
    }
}
