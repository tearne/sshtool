//! Establishing an authenticated SSH session to the remote host, either directly
//! or tunnelled through a jumphost.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use russh::client::{self, Handle};
use russh::keys::ssh_key;

use crate::authentication::{PasswordStore, authenticate};
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

    pub(crate) fn active_mut(&mut self) -> &mut Handle<Client> {
        &mut self.active
    }
}

pub(crate) async fn connect(
    username: &str,
    remote_host: &str,
    port: u16,
    jumphost: Option<&Endpoint>,
) -> Result<Session> {
    let mut passwords = PasswordStore::default();
    connect_with_passwords(username, remote_host, port, jumphost, &mut passwords).await
}

pub(crate) async fn connect_with_passwords(
    username: &str,
    remote_host: &str,
    port: u16,
    jumphost: Option<&Endpoint>,
    passwords: &mut PasswordStore,
) -> Result<Session> {
    match jumphost {
        Some(jumphost) => {
            connect_via_jumphost(username, remote_host, port, jumphost, passwords).await
        }
        None => connect_directly(username, remote_host, port, passwords).await,
    }
}

async fn connect_directly(
    username: &str,
    remote_host: &str,
    port: u16,
    passwords: &mut PasswordStore,
) -> Result<Session> {
    let mut active = open_session(remote_host, port).await?;
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
    let mut handle = open_session(host, port).await?;
    authenticate(&mut handle, username, passwords).await?;
    Ok(handle)
}

async fn connect_via_jumphost(
    username: &str,
    remote_host: &str,
    port: u16,
    jumphost: &Endpoint,
    passwords: &mut PasswordStore,
) -> Result<Session> {
    let jump_username = jumphost.username.as_deref().unwrap_or(username);
    let mut jump = open_session(&jumphost.host, DEFAULT_SSH_PORT).await?;
    authenticate(&mut jump, jump_username, passwords).await?;

    let tunnel = jump
        .channel_open_direct_tcpip(remote_host, port.into(), "127.0.0.1", 0)
        .await
        .with_context(|| format!("opening a tunnel to {remote_host} through {}", jumphost.host))?;

    let mut active = client::connect_stream(ssh_config(), tunnel.into_stream(), Client)
        .await
        .with_context(|| format!("starting an SSH session to {remote_host} over the jumphost"))?;
    authenticate(&mut active, username, passwords).await?;

    Ok(Session {
        active,
        _jump: Some(jump),
    })
}

async fn open_session(host: &str, port: u16) -> Result<Handle<Client>> {
    client::connect(ssh_config(), (host, port), Client)
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
        None => open_session(host, port).await,
        Some(jumphost) => {
            // The jump session must be authenticated to open the tunnel through it.
            // We authenticate the jump with agent-only (no password) since we're
            // verifying that key auth works.
            let mut jump = open_session(&jumphost.host, DEFAULT_SSH_PORT).await?;
            let mut passwords = PasswordStore::default();
            authenticate(&mut jump, jumphost.username.as_deref().unwrap_or(host), &mut passwords).await?;
            let tunnel = jump
                .channel_open_direct_tcpip(host, port.into(), "127.0.0.1", 0)
                .await
                .with_context(|| format!("opening tunnel to {host} through {}", jumphost.host))?;
            client::connect_stream(ssh_config(), tunnel.into_stream(), Client)
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
#[derive(Clone)]
pub(crate) struct Client;

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}
