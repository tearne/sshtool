//! A best-effort local record of connection lifecycle events — connects and
//! drops — so intermittent failures can be read back after the fact. Each drop
//! names which detection signal fired; interpreting local-versus-remote cause is
//! deliberately left to later diagnostics work. Writing never interrupts the
//! session: any IO error is silently dropped.

use std::env;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// The most recent events kept in the log; older lines are trimmed on write.
const MAX_EVENTS: usize = 1000;

/// Which detection signal reported a dropped connection — one per place a live
/// session can notice it has died.
pub(crate) enum DropSignal {
    /// The keepalive heartbeat timed out or errored.
    Heartbeat,
    /// A port-forward accept loop failed.
    Forwarding,
    /// The foreground shell or command read failed.
    Foreground,
}

impl fmt::Display for DropSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DropSignal::Heartbeat => "heartbeat",
            DropSignal::Forwarding => "forwarding",
            DropSignal::Foreground => "foreground",
        })
    }
}

pub(crate) fn connected(host: &str) {
    record(format!("{host} CONNECTED"));
}

pub(crate) fn connect_failed(host: &str, error: &anyhow::Error) {
    record(format!("{host} CONNECT_FAILED {error:#}"));
}

pub(crate) fn dropped(host: &str, signal: DropSignal, error: &anyhow::Error) {
    record(format!("{host} DROPPED {signal} {error:#}"));
}

fn record(event: String) {
    if let Some(line) = timestamped(&event) {
        append_capped(&line);
    }
}

fn timestamped(event: &str) -> Option<String> {
    let now = OffsetDateTime::now_utc().format(&Rfc3339).ok()?;
    Some(format!("{now} {event}"))
}

/// Append one line, keeping only the most recent `MAX_EVENTS`. The whole file is
/// rewritten each time — cheap at this event volume, and it avoids managing
/// rotation files.
fn append_capped(line: &str) {
    let Some(path) = log_path() else { return };
    if let Some(directory) = path.parent() {
        let _ = fs::create_dir_all(directory);
    }
    let mut lines: Vec<String> = fs::read_to_string(&path)
        .map(|text| text.lines().map(String::from).collect())
        .unwrap_or_default();
    lines.push(line.to_string());
    let start = lines.len().saturating_sub(MAX_EVENTS);
    let _ = fs::write(&path, format!("{}\n", lines[start..].join("\n")));
}

fn log_path() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".sshtool").join("drops.log"))
}
