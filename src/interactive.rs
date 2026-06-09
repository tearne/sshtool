//! Interactive terminal session: a remote shell (or a command) over a PTY, with
//! the local terminal wired to it in raw mode and resize forwarded.

use std::env;
use std::io::Read;

use anyhow::Result;
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode};
use russh::client::{Handle, Msg};
use russh::{Channel, ChannelMsg};
use tokio::io::{AsyncWriteExt, stdout};
use tokio::sync::mpsc;

use crate::connection::Client;

/// Open an interactive session — a shell when `command` is `None`, otherwise that
/// command — and wire it to the local terminal until it closes. Returns the
/// remote exit status.
pub(crate) async fn interactive(session: &Handle<Client>, command: Option<&str>) -> Result<u32> {
    let mut channel = session.channel_open_session().await?;
    request_pty(&channel).await?;
    start(&channel, command).await?;

    let _raw = RawMode::enter()?;
    pump(&mut channel).await
}

async fn request_pty(channel: &Channel<Msg>) -> Result<()> {
    let (columns, rows) = terminal::size().unwrap_or((80, 24));
    let term = env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_owned());
    channel
        .request_pty(true, &term, columns as u32, rows as u32, 0, 0, &[])
        .await?;
    Ok(())
}

async fn start(channel: &Channel<Msg>, command: Option<&str>) -> Result<()> {
    match command {
        Some(command) => channel.exec(true, command).await?,
        None => channel.request_shell(true).await?,
    }
    Ok(())
}

/// Shuttle bytes between the local terminal and the channel until the remote
/// closes, forwarding terminal resizes as they happen.
async fn pump(channel: &mut Channel<Msg>) -> Result<u32> {
    let mut local_stdout = stdout();
    let mut local_stdin = spawn_stdin_reader();
    let mut resize = ResizeWatcher::new()?;
    let mut local_stdin_open = true;
    let mut exit_status = 0;

    loop {
        tokio::select! {
            chunk = local_stdin.recv(), if local_stdin_open => match chunk {
                Some(bytes) => channel.data(bytes.as_slice()).await?,
                None => {
                    local_stdin_open = false;
                    channel.eof().await?;
                }
            },
            _ = resize.changed() => {
                let (columns, rows) = terminal::size().unwrap_or((80, 24));
                channel.window_change(columns as u32, rows as u32, 0, 0).await?;
            }
            message = channel.wait() => match message {
                Some(ChannelMsg::Data { ref data }) | Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                    local_stdout.write_all(data).await?;
                    local_stdout.flush().await?;
                }
                Some(ChannelMsg::ExitStatus { exit_status: status }) => exit_status = status,
                Some(_) => {}
                None => break,
            },
        }
    }

    Ok(exit_status)
}

/// Read raw stdin on a dedicated OS thread, forwarding chunks over a channel.
/// We avoid `tokio::io::stdin`, whose blocking read cannot be cancelled: once the
/// remote session ends, that pending read would hang runtime shutdown until the
/// user pressed a key. A plain thread we never join is simply killed at exit.
fn spawn_stdin_reader() -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel::<Vec<u8>>(16);
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buffer = [0u8; 4096];
        loop {
            match stdin.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if sender.blocking_send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    receiver
}

/// Puts the local terminal into raw mode and restores it on drop — so editors,
/// control keys, and full-screen programs work, and an early return or panic
/// still leaves the terminal usable.
struct RawMode;

impl RawMode {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        Ok(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Notices terminal resizes. On Unix that is `SIGWINCH`; on platforms without a
/// resize signal it simply never fires (Windows resize is handled separately,
/// later).
#[cfg(unix)]
struct ResizeWatcher(tokio::signal::unix::Signal);

#[cfg(unix)]
impl ResizeWatcher {
    fn new() -> Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Self(signal(SignalKind::window_change())?))
    }

    async fn changed(&mut self) {
        self.0.recv().await;
    }
}

#[cfg(not(unix))]
struct ResizeWatcher;

#[cfg(not(unix))]
impl ResizeWatcher {
    fn new() -> Result<Self> {
        Ok(Self)
    }

    async fn changed(&mut self) {
        std::future::pending::<()>().await
    }
}
