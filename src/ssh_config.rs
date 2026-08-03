//! Resolving connection parameters from the user's ~/.ssh/config.
//!
//! Handles Host pattern matching and the five directives sshtool uses:
//! HostName, User, Port, ProxyJump, IdentityFile (reserved for future use).

use std::env;
use std::fs;
use std::path::PathBuf;

/// The subset of ssh_config directives that sshtool acts on.
#[derive(Debug, Default, Clone)]
pub struct HostParams {
    /// Resolved hostname to connect to (replaces the alias).
    pub host_name: Option<String>,
    /// Username to use when none is given on the CLI.
    pub user: Option<String>,
    /// Port to connect on.
    pub port: Option<u16>,
    /// ProxyJump targets (comma-separated entries become separate strings).
    pub proxy_jump: Option<Vec<String>>,
    /// LocalForward specs, each normalised to the `[bind:]port:host:hostport`
    /// colon form so they parse identically to a `-L` flag. Accumulates: ssh
    /// applies every matching LocalForward line, not just the first.
    pub local_forwards: Vec<String>,
}

/// Load `~/.ssh/config` and return the parameters that match `host`.
/// Returns empty (all-`None`) params if the file is absent or `host` matches nothing.
pub fn resolve(host: &str) -> HostParams {
    let path = match config_path() {
        Some(p) => p,
        None => return HostParams::default(),
    };
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return HostParams::default(),
    };
    parse(&text, host)
}

fn config_path() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".ssh").join("config"))
}

fn parse(text: &str, target_host: &str) -> HostParams {
    let mut params = HostParams::default();
    let mut in_matching_block = false;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (key, value) = match split_kv(line) {
            Some(kv) => kv,
            None => continue,
        };

        if key.eq_ignore_ascii_case("Host") {
            in_matching_block = patterns_match(value, target_host);
            continue;
        }

        // Lines before any Host block are global defaults — treat as always-matching.
        if !in_matching_block {
            continue;
        }

        if key.eq_ignore_ascii_case("HostName") && params.host_name.is_none() {
            params.host_name = Some(value.to_owned());
        } else if key.eq_ignore_ascii_case("User") && params.user.is_none() {
            params.user = Some(value.to_owned());
        } else if key.eq_ignore_ascii_case("Port") && params.port.is_none() {
            params.port = value.parse().ok();
        } else if key.eq_ignore_ascii_case("ProxyJump") && params.proxy_jump.is_none() {
            let hops: Vec<String> = value.split(',').map(|s| s.trim().to_owned()).collect();
            params.proxy_jump = Some(hops);
        } else if key.eq_ignore_ascii_case("LocalForward") {
            // ssh form is two whitespace-separated tokens: "[bind:]port host:hostport".
            // Joining them with ':' yields the same `[bind:]port:host:hostport` shape
            // a `-L` flag uses, so both feed one parser downstream.
            if let Some((listen, dest)) = value.split_once(char::is_whitespace) {
                params
                    .local_forwards
                    .push(format!("{}:{}", listen.trim(), dest.trim()));
            }
        }
    }

    params
}

/// Split a config line into (keyword, value), handling both `Key Value` and `Key=Value` forms.
fn split_kv(line: &str) -> Option<(&str, &str)> {
    if let Some((k, v)) = line.split_once('=') {
        let k = k.trim();
        let v = v.trim();
        if !k.is_empty() && !v.is_empty() {
            return Some((k, v));
        }
    }
    if let Some((k, v)) = line.split_once(char::is_whitespace) {
        let k = k.trim();
        let v = v.trim();
        if !k.is_empty() && !v.is_empty() {
            return Some((k, v));
        }
    }
    None
}

/// Return true if any of the space-separated patterns in `patterns` matches `host`.
/// Supports `*` (any sequence) and `?` (any single character) wildcards.
/// A leading `!` negates the pattern; a negated match causes an immediate false.
fn patterns_match(patterns: &str, host: &str) -> bool {
    let mut matched = false;
    for pattern in patterns.split_whitespace() {
        if let Some(neg) = pattern.strip_prefix('!') {
            if glob_match(neg, host) {
                return false;
            }
        } else if glob_match(pattern, host) {
            matched = true;
        }
    }
    matched
}

/// Simple glob match: `*` matches any sequence, `?` matches any single character.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_match_chars(&p, &t)
}

fn glob_match_chars(p: &[char], t: &[char]) -> bool {
    match (p.first(), t.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            // * can match zero characters or one more
            glob_match_chars(&p[1..], t)
                || (!t.is_empty() && glob_match_chars(p, &t[1..]))
        }
        (Some('?'), Some(_)) => glob_match_chars(&p[1..], &t[1..]),
        (Some(pc), Some(tc)) if pc.eq_ignore_ascii_case(tc) => glob_match_chars(&p[1..], &t[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
Host bastion
    HostName bastion.example.com
    User deploy
    Port 2222

Host *.internal
    ProxyJump bastion
    User admin

Host multi-jump
    ProxyJump hop1,hop2

Host forwarder
    LocalForward 8080 localhost:80
    LocalForward 127.0.0.1:5432 db.internal:5432

Host negated !bad
    User other
"#;

    #[test]
    fn resolves_exact_alias() {
        let p = parse(SAMPLE, "bastion");
        assert_eq!(p.host_name.as_deref(), Some("bastion.example.com"));
        assert_eq!(p.user.as_deref(), Some("deploy"));
        assert_eq!(p.port, Some(2222));
    }

    #[test]
    fn resolves_wildcard_host() {
        let p = parse(SAMPLE, "db.internal");
        assert_eq!(p.user.as_deref(), Some("admin"));
        assert!(p.proxy_jump.as_deref().is_some());
    }

    #[test]
    fn proxy_jump_single_entry() {
        let p = parse(SAMPLE, "db.internal");
        assert_eq!(p.proxy_jump.as_deref(), Some(["bastion".to_owned()].as_slice()));
    }

    #[test]
    fn proxy_jump_multiple_entries() {
        let p = parse(SAMPLE, "multi-jump");
        assert_eq!(
            p.proxy_jump.as_deref(),
            Some(["hop1".to_owned(), "hop2".to_owned()].as_slice())
        );
    }

    #[test]
    fn no_match_returns_defaults() {
        let p = parse(SAMPLE, "unknown");
        assert!(p.host_name.is_none());
        assert!(p.user.is_none());
        assert!(p.port.is_none());
    }

    #[test]
    fn negation_excludes_host() {
        let p = parse(SAMPLE, "bad");
        assert!(p.user.is_none());
    }

    #[test]
    fn local_forward_accumulates_and_normalises() {
        let p = parse(SAMPLE, "forwarder");
        assert_eq!(
            p.local_forwards,
            vec![
                "8080:localhost:80".to_owned(),
                "127.0.0.1:5432:db.internal:5432".to_owned(),
            ]
        );
    }

    #[test]
    fn no_local_forward_is_empty() {
        let p = parse(SAMPLE, "bastion");
        assert!(p.local_forwards.is_empty());
    }
}
