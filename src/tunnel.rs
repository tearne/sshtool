//! Auto-reconnecting local port forwarding: bind local TCP listeners and tunnel
//! each accepted connection through an SSH channel to the remote endpoint.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

use crate::Target;
use crate::connection::{self, Session};

/// A single `-L local_port:remote_host:remote_port` forward specification.
pub struct LocalForward {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

/// Parse a `-L` argument of the form `local_port:remote_host:remote_port`.
pub fn parse_forward(spec: &str) -> Result<LocalForward> {
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    match parts.as_slice() {
        [local, host, remote] => {
            let local_port = local
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid local port in -L {spec:?}"))?;
            let remote_port = remote
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid remote port in -L {spec:?}"))?;
            if host.is_empty() {
                bail!("missing remote host in -L {spec:?}");
            }
            Ok(LocalForward {
                local_port,
                remote_host: host.to_string(),
                remote_port,
            })
        }
        _ => bail!("-L spec must be local_port:remote_host:remote_port, got {spec:?}"),
    }
}

/// Bind local TCP listeners for each forward and run the reconnect loop until
/// the process is killed. Listeners stay bound across reconnects so the local
/// port is never released.
pub async fn tunnel(target: &Target, forwards: &[LocalForward], verbose: u8) -> Result<()> {
    if verbose >= 3 {
        // russh uses the `log` crate; wire it to stderr at trace level.
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Trace)
            .target(env_logger::Target::Stderr)
            .init();
    }

    let listeners = bind_listeners(forwards).await?;

    for forward in forwards {
        eprintln!(
            "Forwarding localhost:{} → {}:{}",
            forward.local_port, forward.remote_host, forward.remote_port
        );
    }

    let listeners = Arc::new(listeners);
    let mut backoff = Duration::from_secs(2);

    loop {
        if verbose >= 1 {
            eprintln!("Connecting to {}…", target.host);
        }
        match run_session(target, forwards, Arc::clone(&listeners), verbose).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                eprintln!("Connection lost: {err:#}");
                eprintln!("Reconnecting in {}s…", backoff.as_secs());
                sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
        }
    }
}



async fn bind_listeners(forwards: &[LocalForward]) -> Result<Vec<TcpListener>> {
    let mut listeners = Vec::with_capacity(forwards.len());
    for forward in forwards {
        let listener = TcpListener::bind(("127.0.0.1", forward.local_port))
            .await
            .map_err(|e| {
                anyhow::anyhow!("failed to bind localhost:{}: {e}", forward.local_port)
            })?;
        listeners.push(listener);
    }
    Ok(listeners)
}

/// Connect and serve all forwards concurrently until the session fails.
/// Each listener spawns an accept loop as a task; if any task returns an error
/// we cancel the rest and return the error to trigger a reconnect.
async fn run_session(
    target: &Target,
    forwards: &[LocalForward],
    listeners: Arc<Vec<TcpListener>>,
    verbose: u8,
) -> Result<()> {
    let session = Arc::new(connect(target).await?);
    eprintln!("Connected.");

    let mut tasks = tokio::task::JoinSet::new();

    for (i, forward) in forwards.iter().enumerate() {
        let session = Arc::clone(&session);
        let listeners = Arc::clone(&listeners);
        let remote_host = forward.remote_host.clone();
        let remote_port = forward.remote_port;
        let local_port = forward.local_port;

        tasks.spawn(async move {
            accept_loop(&listeners[i], session, &remote_host, remote_port, local_port).await
        });
    }

    // Heartbeat: ping the session periodically and fail fast if it stops responding.
    let heartbeat_session = Arc::clone(&session);
    tasks.spawn(async move { heartbeat(heartbeat_session, verbose).await });

    // Wait for the first accept loop to fail; that signals a dead session.
    // An accept loop returning Ok is unexpected — treat it as a disconnect.
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Err(e)) => return Err(e),
            Ok(Ok(())) => return Err(anyhow::anyhow!("accept loop exited unexpectedly")),
            Err(e) => return Err(anyhow::anyhow!("accept task panicked: {e}")),
        }
    }

    Err(anyhow::anyhow!("all accept loops exited unexpectedly"))
}

/// Accept connections on one listener and spawn a forward task per connection.
async fn accept_loop(
    listener: &TcpListener,
    session: Arc<Session>,
    remote_host: &str,
    remote_port: u16,
    local_port: u16,
) -> Result<()> {
    loop {
        let (stream, _addr) = listener.accept().await?;
        let session = Arc::clone(&session);
        let remote_host = remote_host.to_owned();
        tokio::spawn(async move {
            if let Err(e) =
                forward_connection(stream, &session, &remote_host, remote_port).await
            {
                eprintln!("Forward localhost:{local_port} error: {e:#}");
            }
        });
    }
}

/// Ping the session every 15 seconds; fail if a ping takes longer than 15 seconds
/// to get a reply. This gives the JoinSet an error to catch when the network drops,
/// since the accept loops themselves have no visibility into the session state.
async fn heartbeat(session: Arc<Session>, verbose: u8) -> Result<()> {
    loop {
        sleep(Duration::from_secs(15)).await;
        if verbose >= 2 {
            eprintln!("[vv] sending keepalive…");
        }
        let result = timeout(
            Duration::from_secs(15),
            session.active().send_keepalive(true),
        )
        .await;
        match result {
            Err(_) => {
                eprintln!("[vv] keepalive timed out");
                return Err(anyhow::anyhow!("keepalive timed out — connection appears dead"));
            }
            Ok(Err(e)) => {
                if verbose >= 2 {
                    eprintln!("[vv] keepalive failed: {e}");
                }
                return Err(anyhow::anyhow!("keepalive failed: {e}"));
            }
            Ok(Ok(())) => {
                if verbose >= 2 {
                    eprintln!("[vv] keepalive ok");
                }
            }
        }
    }
}

async fn forward_connection(
    mut local: TcpStream,
    session: &Session,
    remote_host: &str,
    remote_port: u16,
) -> Result<()> {
    let channel = session
        .active()
        .channel_open_direct_tcpip(remote_host, remote_port.into(), "127.0.0.1", 0)
        .await?;
    let mut remote = channel.into_stream();
    copy_bidirectional(&mut local, &mut remote).await?;
    Ok(())
}

async fn connect(target: &Target) -> Result<Session> {
    timeout(
        Duration::from_secs(30),
        connection::connect(&target.username, &target.host, target.port, target.jumphost.as_ref()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timed out connecting to {}", target.host))?
}
