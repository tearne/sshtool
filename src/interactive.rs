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
    let mut stdin = spawn_stdin_reader();
    let _raw = RawMode::enter()?;
    let mut channel = session.channel_open_session().await?;
    request_pty(&channel).await?;
    start(&channel, command).await?;
    pump(&mut channel, &mut stdin).await
}

/// Open an interactive session with reconnection: on drop, wait backoff and
/// reconnect, re-running the same command each time. The stdin reader is shared
/// across reconnects so input typed during a reconnect attempt is not lost.
/// Returns only when the remote command exits cleanly (status 0) or on a
/// non-recoverable error.
pub(crate) async fn reconnecting_interactive(
    target: &crate::Target,
    command: Option<&str>,
    verbose: u8,
) -> Result<u32> {
    let mut stdin = spawn_stdin_reader();
    let _raw = RawMode::enter()?;
    let mut backoff = std::time::Duration::from_secs(2);

    loop {
        if verbose >= 1 {
            eprintln!("Connecting to {}…", target.host);
        }
        let session = match connect_with_timeout(target).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Connection failed: {e:#}");
                eprintln!("Retrying in {}s…", backoff.as_secs());
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(std::time::Duration::from_secs(60));
                continue;
            }
        };
        if verbose >= 1 {
            eprintln!("Connected.");
        }
        backoff = std::time::Duration::from_secs(2);

        let mut channel = match session.active().channel_open_session().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Session error: {e:#}");
                continue;
            }
        };
        if let Err(e) = request_pty(&channel).await {
            eprintln!("PTY error: {e:#}");
            continue;
        }
        if let Err(e) = start(&channel, command).await {
            eprintln!("Start error: {e:#}");
            continue;
        }

        match pump(&mut channel, &mut stdin).await {
            Ok(status) if status == 0 => return Ok(status),
            Ok(status) => return Ok(status),
            Err(e) => {
                eprintln!("Connection lost: {e:#}");
                eprintln!("Reconnecting in {}s…", backoff.as_secs());
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(std::time::Duration::from_secs(60));
            }
        }
    }
}

async fn connect_with_timeout(target: &crate::Target) -> Result<crate::connection::Session> {
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        crate::connection::connect(&target.username, &target.host, target.port, target.jumphost.as_ref()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timed out connecting to {}", target.host))?
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
async fn pump(channel: &mut Channel<Msg>, local_stdin: &mut mpsc::Receiver<Vec<u8>>) -> Result<u32> {
    let mut local_stdout = stdout();
    let mut resize = ResizeWatcher::new()?;
    let mut local_stdin_open = true;
    let mut exit_status: Option<u32> = None;

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
                Some(ChannelMsg::ExitStatus { exit_status: status }) => exit_status = Some(status),
                Some(_) => {}
                None => break,
            },
        }
    }

    // A clean exit sends ExitStatus before closing the channel. No ExitStatus
    // means the channel was closed by a network drop — signal reconnection.
    match exit_status {
        Some(status) => Ok(status),
        None => Err(anyhow::anyhow!("connection dropped")),
    }
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
/// still leaves the terminal usable. On Windows it additionally switches the
/// console to virtual-terminal input and output, so keystrokes reach the remote
/// as a raw VT byte stream and the remote's escape sequences render.
struct RawMode {
    #[cfg(windows)]
    console: windows_console::VirtualTerminal,
}

impl RawMode {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        Ok(RawMode {
            #[cfg(windows)]
            console: windows_console::VirtualTerminal::enable()?,
        })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // Undo our VT flags before crossterm un-raws, so crossterm's restore of
        // the true original input mode has the final say.
        #[cfg(windows)]
        self.console.restore();
        let _ = disable_raw_mode();
    }
}

/// Windows-only console-mode handling. crossterm's raw mode clears echo, line,
/// and processed input but leaves virtual-terminal translation off; without it
/// the threaded stdin reader never sees arrow/function keys as VT sequences and
/// the remote's output escapes print literally. We flip both VT flags ourselves
/// and remember the prior modes so they can be put back.
#[cfg(windows)]
mod windows_console {
    use anyhow::{Result, bail};
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        GetConsoleMode, GetStdHandle, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        SetConsoleMode,
    };

    /// The two console streams we touched, each remembering its original mode.
    pub(super) struct VirtualTerminal {
        input: ConsoleStream,
        output: ConsoleStream,
    }

    impl VirtualTerminal {
        pub(super) fn enable() -> Result<Self> {
            Ok(Self {
                input: ConsoleStream::enable(STD_INPUT_HANDLE, ENABLE_VIRTUAL_TERMINAL_INPUT)?,
                output: ConsoleStream::enable(STD_OUTPUT_HANDLE, ENABLE_VIRTUAL_TERMINAL_PROCESSING)?,
            })
        }

        pub(super) fn restore(&self) {
            self.input.restore();
            self.output.restore();
        }
    }

    struct ConsoleStream {
        stream: STD_HANDLE,
        original: CONSOLE_MODE,
    }

    impl ConsoleStream {
        fn enable(stream: STD_HANDLE, flag: CONSOLE_MODE) -> Result<Self> {
            let original = read_mode(stream)?;
            write_mode(stream, original | flag)?;
            Ok(Self { stream, original })
        }

        fn restore(&self) {
            let _ = write_mode(self.stream, self.original);
        }
    }

    fn read_mode(stream: STD_HANDLE) -> Result<CONSOLE_MODE> {
        let handle = unsafe { GetStdHandle(stream) };
        if handle == INVALID_HANDLE_VALUE {
            bail!("no console on this stream");
        }
        let mut mode: CONSOLE_MODE = 0;
        if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
            bail!("GetConsoleMode failed — not a console");
        }
        Ok(mode)
    }

    fn write_mode(stream: STD_HANDLE, mode: CONSOLE_MODE) -> Result<()> {
        let handle = unsafe { GetStdHandle(stream) };
        if unsafe { SetConsoleMode(handle, mode) } == 0 {
            bail!("SetConsoleMode failed");
        }
        Ok(())
    }
}

/// Notices terminal resizes and lets the pump forward them to the remote. Unix
/// waits on `SIGWINCH`; platforms without a resize signal (Windows) poll the
/// console size instead — see the `not(unix)` implementation below.
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

/// Platforms without a resize signal (Windows) poll the console size instead,
/// since reading raw bytes from stdin rules out crossterm's resize event, which
/// would compete for the same console input.
#[cfg(not(unix))]
struct ResizeWatcher {
    last_size: (u16, u16),
}

#[cfg(not(unix))]
impl ResizeWatcher {
    fn new() -> Result<Self> {
        Ok(Self {
            last_size: terminal::size().unwrap_or((80, 24)),
        })
    }

    /// Sample the size every 200 ms and return once it differs. The pump re-reads
    /// the size to send `window_change`, so detecting a change is all that's needed.
    async fn changed(&mut self) {
        use std::time::Duration;
        let mut tick = tokio::time::interval(Duration::from_millis(200));
        loop {
            tick.tick().await;
            let current = terminal::size().unwrap_or(self.last_size);
            if current != self.last_size {
                self.last_size = current;
                return;
            }
        }
    }
}
