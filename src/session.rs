//! Supervising a live SSH session: run the port forwards and the foreground
//! activity (shell, command, or nothing) together on one connection, and — when
//! asked with `-r` — reconnect on drop. This is the single orchestrator that
//! replaced the separate forward-only, reconnecting-interactive, and one-shot
//! paths, so a session can now forward ports *and* carry a shell at once, as ssh
//! does.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::sleep;

use crate::connection::{self, Session};
use crate::interactive::{self, RawMode, spawn_stdin_reader};
use crate::tunnel::{self, BoundForward, LocalForward};
use crate::{Target, command};

/// What runs in the foreground of the session, alongside the forwards.
pub enum Foreground {
    /// `-N`: no shell or command; the session exists only to serve forwards.
    None,
    /// A remote command streamed without a PTY.
    Command(String),
    /// A shell (when `None`) or a command over a PTY.
    Interactive(Option<String>),
}

/// Bind the forwards, then connect and serve until the foreground finishes or —
/// with `reconnect` — forever across drops. Returns the foreground's exit status.
pub async fn run(
    target: &Target,
    forwards: Vec<LocalForward>,
    foreground: Foreground,
    reconnect: bool,
    verbose: u8,
) -> Result<u32> {
    if verbose >= 3 {
        // russh uses the `log` crate; wire it to stderr at trace level.
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Trace)
            .target(env_logger::Target::Stderr)
            .init();
    }

    let listeners = Arc::new(bind_forwards_or_fail(&forwards, &foreground).await?);
    let terminal = Terminal::for_foreground(&foreground)?;
    supervise(target, listeners, foreground, terminal, reconnect, verbose).await
}

async fn bind_forwards_or_fail(
    forwards: &[LocalForward],
    foreground: &Foreground,
) -> Result<Vec<BoundForward>> {
    let bound = tunnel::bind_forwards(forwards).await;
    // A forward-only session with nothing bound has no reason to connect.
    if !forwards.is_empty() && bound.is_empty() && matches!(foreground, Foreground::None) {
        anyhow::bail!("no forwards could be bound — nothing to do");
    }
    Ok(bound)
}

async fn supervise(
    target: &Target,
    listeners: Arc<Vec<BoundForward>>,
    foreground: Foreground,
    mut terminal: Terminal,
    reconnect: bool,
    verbose: u8,
) -> Result<u32> {
    let announce = verbose >= 1 || !listeners.is_empty();
    let mut backoff = Duration::from_secs(2);

    loop {
        if announce {
            eprintln!("Connecting to {}…", target.host);
        }
        let session = match connect_with_timeout(target).await {
            Ok(session) => session,
            Err(e) if reconnect => {
                eprintln!("Connection failed: {e:#}");
                backoff = wait_and_back_off(backoff).await;
                continue;
            }
            Err(e) => return Err(e),
        };
        if announce {
            eprintln!("Connected.");
        }
        backoff = Duration::from_secs(2);

        match serve(session, &listeners, &foreground, &mut terminal, verbose).await {
            Round::Completed(status) => return Ok(status),
            Round::Dropped(e) if reconnect => {
                eprintln!("Connection lost: {e:#}");
                backoff = wait_and_back_off(backoff).await;
            }
            Round::Dropped(e) => return Err(e),
        }
    }
}

/// The outcome of serving one connection.
enum Round {
    /// The foreground finished cleanly; the whole session ends with this status.
    Completed(u32),
    /// The connection dropped; reconnect if permitted, otherwise fail.
    Dropped(anyhow::Error),
}

/// Serve one connection: run the forward accept loops and the heartbeat as
/// background tasks while the foreground runs inline. Whichever finishes first
/// decides the round — a foreground exit completes it, any background failure
/// (or a foreground error) signals a drop.
async fn serve(
    session: Session,
    listeners: &Arc<Vec<BoundForward>>,
    foreground: &Foreground,
    terminal: &mut Terminal,
    verbose: u8,
) -> Round {
    let session = Arc::new(session);
    let mut background = spawn_forwarding(&session, listeners, verbose);

    tokio::select! {
        outcome = run_foreground(&session, foreground, terminal) => {
            background.abort_all();
            match outcome {
                Ok(status) => Round::Completed(status),
                Err(e) => Round::Dropped(e),
            }
        }
        drop = first_failure(&mut background) => Round::Dropped(drop),
    }
}

fn spawn_forwarding(
    session: &Arc<Session>,
    listeners: &Arc<Vec<BoundForward>>,
    verbose: u8,
) -> JoinSet<Result<()>> {
    let mut background = JoinSet::new();
    for index in 0..listeners.len() {
        let session = Arc::clone(session);
        let listeners = Arc::clone(listeners);
        background.spawn(async move { tunnel::accept_loop(listeners, index, session).await });
    }
    let session = Arc::clone(session);
    background.spawn(async move { tunnel::heartbeat(session, verbose).await });
    background
}

async fn run_foreground(
    session: &Arc<Session>,
    foreground: &Foreground,
    terminal: &mut Terminal,
) -> Result<u32> {
    match foreground {
        Foreground::None => std::future::pending().await,
        Foreground::Command(command) => command::run(session.active(), command).await,
        Foreground::Interactive(command) => {
            interactive::run_once(session.active(), command.as_deref(), terminal.stdin()).await
        }
    }
}

/// Wait for the first background task to end. A background task ending at all
/// means the session is no longer usable — accept loops and the heartbeat only
/// return on error, so any return is a drop signal.
async fn first_failure(background: &mut JoinSet<Result<()>>) -> anyhow::Error {
    match background.join_next().await {
        Some(Ok(Err(e))) => e,
        Some(Ok(Ok(()))) => anyhow::anyhow!("a forwarding task exited unexpectedly"),
        Some(Err(e)) => anyhow::anyhow!("a forwarding task panicked: {e}"),
        None => anyhow::anyhow!("no forwarding tasks running"),
    }
}

async fn connect_with_timeout(target: &Target) -> Result<Session> {
    tokio::time::timeout(
        Duration::from_secs(30),
        connection::connect(&target.username, &target.host, target.port, target.jumphost.as_ref()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timed out connecting to {}", target.host))?
}

async fn wait_and_back_off(backoff: Duration) -> Duration {
    eprintln!("Reconnecting in {}s…", backoff.as_secs());
    sleep(backoff).await;
    (backoff * 2).min(Duration::from_secs(60))
}

/// The local terminal state an interactive foreground needs: raw mode held for
/// the whole session and one stdin reader shared across reconnects, so input
/// typed during a reconnect isn't lost. Non-interactive foregrounds hold nothing.
struct Terminal {
    _raw: Option<RawMode>,
    stdin: Option<mpsc::Receiver<Vec<u8>>>,
}

impl Terminal {
    fn for_foreground(foreground: &Foreground) -> Result<Self> {
        if matches!(foreground, Foreground::Interactive(_)) {
            Ok(Self {
                _raw: Some(RawMode::enter()?),
                stdin: Some(spawn_stdin_reader()),
            })
        } else {
            Ok(Self { _raw: None, stdin: None })
        }
    }

    fn stdin(&mut self) -> &mut mpsc::Receiver<Vec<u8>> {
        self.stdin
            .as_mut()
            .expect("interactive foreground must have a stdin reader")
    }
}
