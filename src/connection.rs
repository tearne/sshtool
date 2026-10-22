//! Establishing an authenticated SSH session to the remote host, either directly
//! or tunnelled through a jumphost.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use russh::Channel;
use russh::client::{self, Handle, Msg};
use russh::keys::ssh_key;
use tokio::io::copy_bidirectional;

use crate::authentication::{PasswordStore, authenticate, connect_agent_socket};
use crate::endpoint::Endpoint;

pub(crate) const DEFAULT_SSH_PORT: u16 = 22;

/// An authenticated session against the remote host. When a jumphost is used the
/// jump session is kept alive for the lifetime of this value — the remote session
/// rides on a channel through it, so dropping the jump would tear the tunnel down.
pub(crate) struct Session {
    active: Handle<Client>,
    _jump: Option<Handle<Client>>,
}

impl Session {
    pub(crate) fn active(&self) -> &Handle<Client> {
        &self.active
    }
}

pub(crate) async fn connect(
    username: &str,
    remote_host: &str,
    port: u16,
    jumphost: Option<&Endpoint>,
    destination: Client,
) -> Result<Session> {
    let mut passwords = PasswordStore::default();
    connect_with_passwords(username, remote_host, port, jumphost, destination, &mut passwords).await
}

pub(crate) async fn connect_with_passwords(
    username: &str,
    remote_host: &str,
    port: u16,
    jumphost: Option<&Endpoint>,
    destination: Client,
    passwords: &mut PasswordStore,
) -> Result<Session> {
    match jumphost {
        Some(jumphost) => {
            connect_via_jumphost(username, remote_host, port, jumphost, destination, passwords).await
        }
        None => connect_directly(username, remote_host, port, destination, passwords).await,
    }
}

async fn connect_directly(
    username: &str,
    remote_host: &str,
    port: u16,
    destination: Client,
    passwords: &mut PasswordStore,
) -> Result<Session> {
    let mut active = open_session(remote_host, port, destination).await?;
    authenticate(&mut active, username, passwords).await?;
    Ok(Session {
        active,
        _jump: None,
    })
}

/// Open and authenticate a direct session, returning the raw handle.
/// Used by setup to authenticate each hop independently.
pub(crate) async fn open_and_authenticate(
    host: &str,
    port: u16,
    username: &str,
    passwords: &mut PasswordStore,
) -> Result<Handle<Client>> {
    let mut handle = open_session(host, port, Client::default()).await?;
    authenticate(&mut handle, username, passwords).await?;
    Ok(handle)
}

async fn connect_via_jumphost(
    username: &str,
    remote_host: &str,
    port: u16,
    jumphost: &Endpoint,
    destination: Client,
    passwords: &mut PasswordStore,
) -> Result<Session> {
    let jump_username = jumphost.username.as_deref().unwrap_or(username);
    let mut jump = open_session(&jumphost.host, DEFAULT_SSH_PORT, Client::default()).await?;
    authenticate(&mut jump, jump_username, passwords).await?;

    let tunnel = jump
        .channel_open_direct_tcpip(remote_host, port.into(), "127.0.0.1", 0)
        .await
        .with_context(|| format!("opening a tunnel to {remote_host} through {}", jumphost.host))?;

    let mut active = client::connect_stream(ssh_config(), tunnel.into_stream(), destination)
        .await
        .with_context(|| format!("starting an SSH session to {remote_host} over the jumphost"))?;
    authenticate(&mut active, username, passwords).await?;

    Ok(Session {
        active,
        _jump: Some(jump),
    })
}

async fn open_session(host: &str, port: u16, handler: Client) -> Result<Handle<Client>> {
    client::connect(ssh_config(), (host, port), handler)
        .await
        .with_context(|| format!("connecting to {host} (is the host name correct and reachable?)"))
}

/// Open an unauthenticated session handle, routed through a jumphost if given.
/// Used for verification connections where we want to authenticate with the agent only.
pub(crate) async fn open_unauthenticated(
    host: &str,
    port: u16,
    jumphost: Option<&Endpoint>,
) -> Result<Handle<Client>> {
    match jumphost {
        None => open_session(host, port, Client::default()).await,
        Some(jumphost) => {
            // The jump session must be authenticated to open the tunnel through it.
            // We authenticate the jump with agent-only (no password) since we're
            // verifying that key auth works.
            let mut jump = open_session(&jumphost.host, DEFAULT_SSH_PORT, Client::default()).await?;
            let mut passwords = PasswordStore::default();
            authenticate(&mut jump, jumphost.username.as_deref().unwrap_or(host), &mut passwords).await?;
            let tunnel = jump
                .channel_open_direct_tcpip(host, port.into(), "127.0.0.1", 0)
                .await
                .with_context(|| format!("opening tunnel to {host} through {}", jumphost.host))?;
            client::connect_stream(ssh_config(), tunnel.into_stream(), Client::default())
                .await
                .with_context(|| format!("starting SSH session to {host} over jumphost"))
        }
    }
}

pub(crate) fn ssh_config() -> Arc<client::Config> {
    Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        ..<client::Config>::default()
    })
}

/// The SSH event handler. For this demonstrator we accept any host key — host key
/// verification is deliberately out of scope and a security gap to revisit.
///
/// Agent channels are bridged only on a connection built with
/// [`Client::forwarding_agent`]; on any other (jumphosts included) a server-opened
/// agent channel is refused, so no hop reaches the local agent unasked — as in
/// OpenSSH.
#[derive(Clone, Default)]
pub(crate) struct Client {
    forward_agent: bool,
    verbose: u8,
}

impl Client {
    pub(crate) fn forwarding_agent(verbose: u8) -> Self {
        Self { forward_agent: true, verbose }
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn server_channel_open_agent_forward(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if !self.forward_agent {
            if self.verbose >= 1 {
                eprintln!("[v] refused an agent channel the server opened unrequested");
            }
            return channel.close().await;
        }
        tokio::spawn(bridge_to_local_agent(channel, self.verbose));
        Ok(())
    }
}

/// Splice one forwarded agent channel onto its own connection to the local
/// agent, so concurrent remote clients each get an independent agent session.
async fn bridge_to_local_agent(channel: Channel<Msg>, verbose: u8) {
    let mut agent = match connect_agent_socket().await {
        Ok(agent) => agent,
        Err(e) => {
            if verbose >= 1 {
                eprintln!("[v] agent forwarding: local agent unreachable: {e}");
            }
            let _ = channel.close().await;
            return;
        }
    };
    let mut remote = channel.into_stream();
    if let Err(e) = copy_bidirectional(&mut remote, &mut agent).await
        && verbose >= 1
    {
        eprintln!("[v] agent forwarding channel ended: {e}");
    }
}

/// Ask the server to offer agent forwarding on `channel`, before the shell or
/// command starts so the remote environment gets `SSH_AUTH_SOCK`. A refusal is
/// logged at `-v` and the session carries on without it.
pub(crate) async fn request_agent_forwarding(channel: &mut Channel<Msg>, verbose: u8) -> Result<()> {
    channel.agent_forward(true).await?;
    loop {
        match channel.wait().await {
            Some(russh::ChannelMsg::Success) => return Ok(()),
            Some(russh::ChannelMsg::Failure) => {
                if verbose >= 1 {
                    eprintln!("[v] server refused agent forwarding; continuing without it");
                }
                return Ok(());
            }
            Some(_) => {}
            None => anyhow::bail!("channel closed while requesting agent forwarding"),
        }
    }
}
