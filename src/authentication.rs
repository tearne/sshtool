//! Authenticating a hop the way SSH itself would: try keys held by a running
//! ssh-agent first, then fall back to prompting for a password.

use std::collections::HashMap;

use anyhow::{Result, bail};
use russh::client::Handle;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};

use crate::connection::Client;

/// Passwords entered during one connection, remembered by username so the same
/// account isn't prompted again for a later hop. Lives for a single `connect`;
/// never persisted.
#[derive(Default, Clone)]
pub(crate) struct PasswordStore {
    by_username: HashMap<String, String>,
}

impl PasswordStore {
    fn remembered(&self, username: &str) -> Option<String> {
        self.by_username.get(username).cloned()
    }

    fn remember(&mut self, username: &str, password: String) {
        self.by_username.insert(username.to_owned(), password);
    }
}

pub(crate) async fn authenticate(
    session: &mut Handle<Client>,
    username: &str,
    passwords: &mut PasswordStore,
) -> Result<()> {
    if authenticate_with_agent(session, username).await? {
        return Ok(());
    }
    authenticate_with_password(session, username, passwords).await
}

/// Returns `true` if an agent key authenticated us. A missing agent is not an
/// error — it just means we fall through to the password prompt.
async fn authenticate_with_agent(session: &mut Handle<Client>, username: &str) -> Result<bool> {
    match connect_to_agent().await {
        Some(agent) => try_agent_keys(session, username, agent).await,
        None => Ok(false),
    }
}

/// Offer each public key the agent holds until one authenticates. The agent does
/// the signing, so the private keys never leave it.
async fn try_agent_keys<S>(
    session: &mut Handle<Client>,
    username: &str,
    mut agent: AgentClient<S>,
) -> Result<bool>
where
    S: AgentStream + Send + Unpin,
{
    let identities = agent.request_identities().await.unwrap_or_default();
    for identity in identities {
        if let AgentIdentity::PublicKey { key, .. } = identity {
            let outcome = session
                .authenticate_publickey_with(username, key, None, &mut agent)
                .await?;
            if outcome.success() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Connect to the platform's ssh-agent: the `SSH_AUTH_SOCK` socket on Unix, the
/// OpenSSH named pipe on Windows. `None` means no agent is reachable.
#[cfg(unix)]
async fn connect_to_agent() -> Option<AgentClient<tokio::net::UnixStream>> {
    AgentClient::connect_env().await.ok()
}

#[cfg(windows)]
async fn connect_to_agent() -> Option<AgentClient<tokio::net::windows::named_pipe::NamedPipeClient>>
{
    AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
        .await
        .ok()
}

/// Try a password already entered for this username; if there is none or it
/// fails, prompt once and try that, remembering it for later hops.
async fn authenticate_with_password(
    session: &mut Handle<Client>,
    username: &str,
    passwords: &mut PasswordStore,
) -> Result<()> {
    if let Some(remembered) = passwords.remembered(username)
        && try_password(session, username, &remembered).await?
    {
        return Ok(());
    }

    let entered = prompt_for_password(username)?;
    if try_password(session, username, &entered).await? {
        passwords.remember(username, entered);
        Ok(())
    } else {
        bail!("authentication failed for {username}");
    }
}

async fn try_password(session: &mut Handle<Client>, username: &str, password: &str) -> Result<bool> {
    let outcome = session
        .authenticate_password(username, password.to_owned())
        .await?;
    Ok(outcome.success())
}

fn prompt_for_password(username: &str) -> Result<String> {
    Ok(rpassword::prompt_password(format!("Password for {username}: "))?)
}
