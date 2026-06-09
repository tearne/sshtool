//! Parsing an SSH-style `[user@]host` endpoint.

use anyhow::{Result, bail};

/// An SSH endpoint: a host, optionally qualified with a username.
pub struct Endpoint {
    pub username: Option<String>,
    pub host: String,
}

pub fn parse_endpoint(text: &str) -> Result<Endpoint> {
    let (username, host) = match text.split_once('@') {
        Some((username, host)) => (Some(username), host),
        None => (None, text),
    };
    if host.is_empty() || username == Some("") {
        bail!("endpoint must be in the form [user@]host, got {text:?}");
    }
    Ok(Endpoint {
        username: username.map(str::to_owned),
        host: host.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_endpoint;

    #[test]
    fn splits_username_from_host() {
        let endpoint = parse_endpoint("alice@server.example").unwrap();
        assert_eq!(endpoint.username.as_deref(), Some("alice"));
        assert_eq!(endpoint.host, "server.example");
    }

    #[test]
    fn accepts_a_bare_host_without_a_username() {
        let endpoint = parse_endpoint("server.example").unwrap();
        assert_eq!(endpoint.username, None);
        assert_eq!(endpoint.host, "server.example");
    }

    #[test]
    fn keeps_only_the_first_at_with_the_username() {
        let endpoint = parse_endpoint("alice@host@weird").unwrap();
        assert_eq!(endpoint.username.as_deref(), Some("alice"));
        assert_eq!(endpoint.host, "host@weird");
    }

    #[test]
    fn rejects_an_empty_username() {
        assert!(parse_endpoint("@host").is_err());
    }

    #[test]
    fn rejects_an_empty_host() {
        assert!(parse_endpoint("alice@").is_err());
    }

    #[test]
    fn rejects_empty_input() {
        assert!(parse_endpoint("").is_err());
    }
}
