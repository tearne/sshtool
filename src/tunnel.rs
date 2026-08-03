//! Local port forwarding mechanics: parse a forward spec, bind its local
//! listener, and shuttle each accepted connection through an SSH channel to the
//! remote endpoint. Reconnection and coordination with a foreground session live
//! in `session`; this module only knows how to move bytes for the forwards.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

use crate::connection::Session;

/// A single local forward: bind `bind_address:local_port` and tunnel to
/// `remote_host:remote_port` on the far side of the SSH connection.
pub struct LocalForward {
    pub bind_address: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

/// A forward whose local listener is bound. The listener stays bound across
/// reconnects so the local port is never released between sessions.
pub(crate) struct BoundForward {
    listener: TcpListener,
    remote_host: String,
    remote_port: u16,
    local_port: u16,
}

/// Parse a forward argument in ssh's `-L` colon form: either
/// `local_port:remote_host:remote_port` or
/// `bind_address:local_port:remote_host:remote_port`. The bind address defaults
/// to `127.0.0.1` when omitted. (IPv6 literal bind addresses are not supported.)
pub fn parse_forward(spec: &str) -> Result<LocalForward> {
    let parts: Vec<&str> = spec.split(':').collect();
    let (bind_address, local, host, remote) = match parts.as_slice() {
        [local, host, remote] => ("127.0.0.1", *local, *host, *remote),
        [bind, local, host, remote] => (*bind, *local, *host, *remote),
        _ => bail!(
            "forward must be [bind_address:]local_port:remote_host:remote_port, got {spec:?}"
        ),
    };

    let local_port = local
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid local port in forward {spec:?}"))?;
    let remote_port = remote
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid remote port in forward {spec:?}"))?;
    if host.is_empty() {
        bail!("missing remote host in forward {spec:?}");
    }
    if bind_address.is_empty() {
        bail!("missing bind address in forward {spec:?}");
    }

    Ok(LocalForward {
        bind_address: bind_address.to_owned(),
        local_port,
        remote_host: host.to_owned(),
        remote_port,
    })
}

/// Bind a local listener for each forward. Binding is per-forward and non-fatal:
/// a forward that cannot bind (privileged low port, address in use, address not
/// local) is reported and skipped so the rest — and the foreground session —
/// carry on, mirroring ssh's default `ExitOnForwardFailure no`.
pub(crate) async fn bind_forwards(forwards: &[LocalForward]) -> Vec<BoundForward> {
    let mut bound = Vec::with_capacity(forwards.len());
    for forward in forwards {
        match TcpListener::bind((forward.bind_address.as_str(), forward.local_port)).await {
            Ok(listener) => {
                eprintln!(
                    "Forwarding {}:{} → {}:{}",
                    forward.bind_address,
                    forward.local_port,
                    forward.remote_host,
                    forward.remote_port
                );
                bound.push(BoundForward {
                    listener,
                    remote_host: forward.remote_host.clone(),
                    remote_port: forward.remote_port,
                    local_port: forward.local_port,
                });
            }
            Err(e) => eprintln!(
                "Warning: not forwarding {}:{} → {}:{}: {e}",
                forward.bind_address, forward.local_port, forward.remote_host, forward.remote_port
            ),
        }
    }
    bound
}

/// Accept connections on one bound forward and spawn a tunnel task per
/// connection. Returns only on a listener error, which the supervisor treats as
/// a dead session.
pub(crate) async fn accept_loop(
    bound: Arc<Vec<BoundForward>>,
    index: usize,
    session: Arc<Session>,
) -> Result<()> {
    let forward = &bound[index];
    loop {
        let (stream, _addr) = forward.listener.accept().await?;
        let session = Arc::clone(&session);
        let remote_host = forward.remote_host.clone();
        let remote_port = forward.remote_port;
        let local_port = forward.local_port;
        tokio::spawn(async move {
            if let Err(e) = forward_connection(stream, &session, &remote_host, remote_port).await {
                eprintln!("Forward localhost:{local_port} error: {e:#}");
            }
        });
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

/// Ping the session every 15 seconds; fail if a ping takes longer than 15 seconds
/// to get a reply. This gives the supervisor an error to catch when the network
/// drops, since the accept loops themselves have no visibility into session state.
pub(crate) async fn heartbeat(session: Arc<Session>, verbose: u8) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_field_form_with_default_bind() {
        let f = parse_forward("8080:localhost:80").unwrap();
        assert_eq!(f.bind_address, "127.0.0.1");
        assert_eq!(f.local_port, 8080);
        assert_eq!(f.remote_host, "localhost");
        assert_eq!(f.remote_port, 80);
    }

    #[test]
    fn parses_four_field_form_with_explicit_bind() {
        let f = parse_forward("0.0.0.0:5432:db.internal:5432").unwrap();
        assert_eq!(f.bind_address, "0.0.0.0");
        assert_eq!(f.local_port, 5432);
        assert_eq!(f.remote_host, "db.internal");
        assert_eq!(f.remote_port, 5432);
    }

    #[test]
    fn rejects_too_few_fields() {
        assert!(parse_forward("8080:localhost").is_err());
    }

    #[test]
    fn rejects_bad_ports() {
        assert!(parse_forward("notaport:localhost:80").is_err());
        assert!(parse_forward("8080:localhost:notaport").is_err());
    }
}
