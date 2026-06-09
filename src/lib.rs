//! Reach a remote host over SSH — directly or through a jumphost — and either run
//! a command (streaming its output) or open an interactive terminal.

mod authentication;
mod command;
mod connection;
mod endpoint;
mod interactive;

pub use endpoint::{Endpoint, parse_endpoint};

use anyhow::Result;

/// A remote destination. The jumphost, when present, may authenticate with its
/// own username; if it carries none it reuses the destination's.
pub struct Target {
    pub username: String,
    pub host: String,
    pub jumphost: Option<Endpoint>,
}

/// Run a command non-interactively, streaming its output to the local terminal,
/// and return the remote exit status.
pub async fn run(target: &Target, command: &str) -> Result<u32> {
    let session = connect(target).await?;
    command::run(session.active(), command).await
}

/// Open an interactive terminal — a shell when `command` is `None`, otherwise that
/// command over a PTY — and return the remote exit status.
pub async fn interactive(target: &Target, command: Option<&str>) -> Result<u32> {
    let session = connect(target).await?;
    interactive::interactive(session.active(), command).await
}

async fn connect(target: &Target) -> Result<connection::Session> {
    connection::connect(&target.username, &target.host, target.jumphost.as_ref()).await
}
