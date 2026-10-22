//! SSH key provisioning: `keygen` to get exactly one key into the agent,
//! `copy_id` to install whatever the agent holds onto every hop.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use russh::keys::agent::AgentIdentity;
use russh::keys::ssh_key::{Algorithm, LineEnding};
use russh::keys::PrivateKey;

use crate::Target;
use crate::authentication::{PasswordStore, connect_to_agent};
use crate::command;
use crate::connection::{self, Client, DEFAULT_SSH_PORT};

// ── keygen ────────────────────────────────────────────────────────────────────

/// Ensure exactly one key is loaded in the agent, generating one if needed.
pub async fn keygen() -> Result<()> {
    let agent_keys = agent_identities().await;

    match agent_keys.as_deref() {
        Ok([identity]) => {
            let fingerprint = public_key_of(identity).fingerprint(Default::default());
            println!("Agent already has a key: {fingerprint}");
            println!("Run `sshtool copy-id user@host` to install it on a remote host.");
            return Ok(());
        }
        Ok(keys) if !keys.is_empty() => {
            println!("Agent has {} keys loaded — cannot proceed safely.", keys.len());
            println!("Run `ssh-add -D` to clear all keys, then run `sshtool keygen` again.");
            return Ok(());
        }
        _ => {} // no agent or empty — fall through to disk check
    }

    let key_files = find_key_files();
    match key_files.as_slice() {
        [] => generate_and_load().await,
        [key_path] => {
            println!("Found existing key: {}", key_path.display());
            if prompt_yes_no("Load into agent? [Y/n] ")? {
                load_key_into_agent(key_path).await?;
                println!("Key loaded. Run `sshtool copy-id user@host` to install it.");
            }
            Ok(())
        }
        paths => {
            println!("Found {} key files — cannot proceed safely:", paths.len());
            for p in paths {
                println!("  {}", p.display());
            }
            println!("Remove all but one, then run `sshtool keygen` again.");
            Ok(())
        }
    }
}

async fn generate_and_load() -> Result<()> {
    let passphrase = prompt_passphrase_twice()?;

    println!("Generating Ed25519 keypair…");
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .context("failed to generate keypair")?;

    let key_to_write = if passphrase.is_empty() {
        key.clone()
    } else {
        key.encrypt(&mut rand::rng(), passphrase.as_bytes())
            .context("failed to encrypt private key")?
    };

    let key_path = default_key_path();
    let pub_path = key_path.with_extension("pub");

    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    key_to_write
        .write_openssh_file(&key_path, LineEnding::LF)
        .with_context(|| format!("failed to write {}", key_path.display()))?;
    let pub_line = key
        .public_key()
        .to_openssh()
        .context("failed to serialise public key")?;
    std::fs::write(&pub_path, format!("{pub_line}\n"))
        .with_context(|| format!("failed to write {}", pub_path.display()))?;

    println!("  Private key: {}", key_path.display());
    println!("  Public key:  {}", pub_path.display());

    // Load the unencrypted key directly — no need to re-read and re-decrypt from disk.
    add_key_to_agent(&key).await?;
    println!("Key loaded into agent.");
    println!("Run `sshtool copy-id user@host` to install it on a remote host.");
    Ok(())
}

async fn load_key_into_agent(key_path: &std::path::Path) -> Result<()> {
    let key = match PrivateKey::read_openssh_file(key_path) {
        Ok(k) if !k.is_encrypted() => k,
        _ => {
            let passphrase = rpassword::prompt_password("Passphrase for key: ")?;
            PrivateKey::read_openssh_file(key_path)
                .context("failed to read key")?
                .decrypt(passphrase.as_bytes())
                .context("failed to decrypt key — wrong passphrase?")?
        }
    };
    add_key_to_agent(&key).await
}

async fn add_key_to_agent(key: &PrivateKey) -> Result<()> {
    match connect_to_agent().await {
        Some(mut agent) => agent
            .add_identity(key, &[])
            .await
            .context("failed to add key to agent"),
        None => bail!("no SSH agent found — is one running?"),
    }
}

// ── copy-id ───────────────────────────────────────────────────────────────────

/// Install the agent's single key on every hop to `target`.
pub async fn copy_id(target: &Target) -> Result<()> {
    let agent_keys = agent_identities().await;

    let identity = match agent_keys.as_deref() {
        Ok([identity]) => identity.clone(),
        Ok(keys) if keys.is_empty() => {
            bail!("no keys in agent — run `sshtool keygen` first");
        }
        Ok(keys) => {
            bail!(
                "agent has {} keys loaded — run `ssh-add -D` to clear, then `sshtool keygen`",
                keys.len()
            );
        }
        Err(_) => bail!("no SSH agent found — run `sshtool keygen` first"),
    };

    let public_key_line = public_key_of(&identity)
        .to_openssh()
        .context("failed to serialise public key")?;

    let mut passwords = PasswordStore::default();

    if let Some(jumphost) = &target.jumphost {
        let jump_username = jumphost.username.as_deref().unwrap_or(&target.username);
        println!("Installing key on jumphost {}…", jumphost.host);
        let handle =
            connection::open_and_authenticate(&jumphost.host, DEFAULT_SSH_PORT, jump_username, &mut passwords)
                .await?;
        install_on_handle(&handle, &public_key_line).await?;
        println!("  Verifying…");
        verify_and_report(jump_username, &jumphost.host, DEFAULT_SSH_PORT, None).await;
    }

    println!("Installing key on {}…", target.host);
    let session = connection::connect_with_passwords(
        &target.username,
        &target.host,
        target.port,
        target.jumphost.as_ref(),
        Client::default(),
        &mut passwords,
    ).await?;
    install_on_handle(session.active(), &public_key_line).await?;
    println!("  Verifying…");
    verify_and_report(&target.username, &target.host, target.port, target.jumphost.as_ref()).await;

    Ok(())
}

async fn install_on_handle(
    handle: &russh::client::Handle<Client>,
    public_key_line: &str,
) -> Result<()> {
    // Use printf|tee rather than echo>> so this works on restricted shells (rbash)
    // that block output redirection. umask 077 sets correct permissions without
    // needing chmod, which also avoids redirection. The grep guard makes the
    // install idempotent — re-running copy-id won't duplicate the key line; we
    // touch the file first so grep has something to read without an error we'd
    // otherwise have to redirect away (also blocked on rbash).
    let cmd = format!(
        "umask 077; mkdir -p .ssh; touch .ssh/authorized_keys; \
         grep -qxF {pubkey} .ssh/authorized_keys || printf '%s\\n' {pubkey} | tee -a .ssh/authorized_keys",
        pubkey = shell_escape(public_key_line),
    );
    let status = command::run(handle, &cmd, false, 0).await?;
    if status != 0 {
        bail!("key install command exited with status {status}");
    }
    Ok(())
}

async fn verify_and_report(username: &str, host: &str, port: u16, jumphost: Option<&crate::endpoint::Endpoint>) {
    match verify_with_fresh_connection(username, host, port, jumphost).await {
        Ok(true) => println!("  ✓ Key authentication works on {host}"),
        Ok(false) => println!(
            "  ✗ Key authentication failed on {host} — check authorized_keys settings"
        ),
        Err(e) => println!("  ✗ Verification error on {host}: {e:#}"),
    }
}

/// Open a fresh connection authenticating with the agent only (no password fallback).
/// Re-authenticating on an existing session hangs on most servers.
async fn verify_with_fresh_connection(
    username: &str,
    host: &str,
    port: u16,
    jumphost: Option<&crate::endpoint::Endpoint>,
) -> Result<bool> {
    let mut handle = connection::open_unauthenticated(host, port, jumphost).await?;
    match connect_to_agent().await {
        Some(mut agent) => {
            let identities = agent.request_identities().await.unwrap_or_default();
            for identity in identities {
                if let AgentIdentity::PublicKey { key, .. } = identity {
                    let outcome = handle
                        .authenticate_publickey_with(username, key, None, &mut agent)
                        .await?;
                    if outcome.success() {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        None => bail!("no SSH agent available for verification"),
    }
}

// ── agent helpers ─────────────────────────────────────────────────────────────

async fn agent_identities() -> Result<Vec<AgentIdentity>> {
    match connect_to_agent().await {
        Some(mut agent) => agent.request_identities().await.map_err(Into::into),
        None => bail!("no SSH agent found"),
    }
}

fn public_key_of(identity: &AgentIdentity) -> russh::keys::ssh_key::PublicKey {
    match identity {
        AgentIdentity::PublicKey { key, .. } => key.clone(),
        AgentIdentity::Certificate { certificate, .. } => {
            russh::keys::ssh_key::PublicKey::new(
                certificate.public_key().clone(),
                certificate.comment(),
            )
        }
    }
}

// ── disk helpers ──────────────────────────────────────────────────────────────

fn find_key_files() -> Vec<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_owned());
    let ssh_dir = PathBuf::from(home).join(".ssh");
    let Ok(entries) = std::fs::read_dir(&ssh_dir) else {
        return vec![];
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            // Must not be a .pub file itself
            p.extension().map(|e| e != "pub").unwrap_or(true)
            // Must have a matching .pub alongside it
            && p.with_extension("pub").exists()
            // Must look like an OpenSSH private key
            && looks_like_openssh_key(p)
        })
        .collect()
}

fn looks_like_openssh_key(path: &std::path::Path) -> bool {
    use std::io::BufRead;
    std::fs::File::open(path)
        .map(|f| std::io::BufReader::new(f).lines().next())
        .ok()
        .flatten()
        .and_then(|l| l.ok())
        .map(|line| line.trim() == "-----BEGIN OPENSSH PRIVATE KEY-----")
        .unwrap_or(false)
}

fn default_key_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(home).join(".ssh").join("id_ed25519")
}

// ── prompt helpers ────────────────────────────────────────────────────────────

fn prompt_yes_no(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    Ok(trimmed.is_empty() || trimmed == "y" || trimmed == "yes")
}

fn prompt_passphrase_twice() -> Result<String> {
    loop {
        let pass1 = rpassword::prompt_password("Enter passphrase (empty for no passphrase): ")?;
        let pass2 = rpassword::prompt_password("Enter same passphrase again: ")?;
        if pass1 == pass2 {
            return Ok(pass1);
        }
        println!("Passphrases do not match. Try again.");
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}
