//! Reach a remote host over SSH — directly or through a jumphost — and either run
//! a command (streaming its output) or open an interactive terminal.

mod authentication;
mod command;
mod connection;
mod diagnostics;
mod endpoint;
mod interactive;
mod key_install;
mod session;
pub mod ssh_config;
pub mod tunnel;
#[cfg(windows)]
mod install;

pub use endpoint::{Endpoint, parse_endpoint};
pub use key_install::{copy_id, keygen};
pub use session::{Foreground, run};
#[cfg(windows)]
pub use install::install;

/// A remote destination. The jumphost, when present, may authenticate with its
/// own username; if it carries none it reuses the destination's.
pub struct Target {
    pub username: String,
    pub host: String,
    pub port: u16,
    pub jumphost: Option<Endpoint>,
}
