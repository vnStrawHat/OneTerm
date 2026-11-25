//! The server's key against the known hosts (specs/oneterm, "M7 in detail"): a known one goes on,
//! one never seen is asked, a changed one is refused with where the key known is.

use std::path::PathBuf;
use std::sync::Arc;

use russh::client::{ChannelOpenHandle, Handler, Msg, Session};
use russh::keys::known_hosts::{check_known_hosts_path, learn_known_hosts_path};
use russh::keys::{HashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelOpenFailure};

use crate::auth::CANCELLED;
use crate::prompt::{Ask, Prompt};

pub(crate) struct Checker {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) file: PathBuf,
    pub(crate) prompt: Arc<Prompt>,
    /// The connection's forwards: the remote ones say where a channel the server opens back
    /// goes.
    pub(crate) remotes: Vec<crate::Forward>,
}

/// The key the known hosts hold for `host` on `port`, before connecting: its kind and its
/// SHA-256 fingerprint, `ED25519 SHA256:k3Vq…9fTn` (the middle left out); None when it is
/// not there.
pub fn known_key(host: &str, port: u16, file: &std::path::Path) -> Option<String> {
    let keys = russh::keys::known_hosts::known_host_keys_path(host, port, file).ok()?;
    let (_, key) = keys.into_iter().next()?;
    Some(said(&key))
}

/// A key's kind and short fingerprint, `ED25519 SHA256:k3Vq…9fTn`.
fn said(key: &russh::keys::PublicKey) -> String {
    let kind = key.algorithm().to_string();
    let kind = kind.strip_prefix("ssh-").unwrap_or(&kind).split('-').next().unwrap_or("").to_uppercase();
    format!("{kind} {}", short_print(&key.fingerprint(HashAlg::Sha256).to_string()))
}

/// A private key file's kind and fingerprint, and whether a passphrase locks it (read without
/// it: OpenSSH keeps the public half in the clear); else why it cannot be read.
pub fn key_info(file: &std::path::Path) -> Result<(String, bool), String> {
    let key = russh::keys::PrivateKey::read_openssh_file(file).map_err(|e| match e {
        russh::keys::ssh_key::Error::Io(_) => "Not found".to_string(),
        _ => "Not an OpenSSH private key".to_string(),
    })?;
    Ok((said(key.public_key()), key.is_encrypted()))
}

/// `SHA256:` and the first and last four of the hash, an ellipsis between.
pub fn short_print(print: &str) -> String {
    let (head, hash) = print.split_once(':').unwrap_or(("", print));
    let chars: Vec<char> = hash.chars().collect();
    if chars.len() <= 10 {
        return print.to_string();
    }
    let (first, last): (String, String) =
        (chars[..4].iter().collect(), chars[chars.len() - 4..].iter().collect());
    format!("{head}:{first}\u{2026}{last}")
}

impl Checker {
    /// Refuses the key: `why` is the error the connection fails with.
    fn refuse(&self, why: String) -> Result<bool, russh::Error> {
        *self.prompt.why.lock().unwrap_or_else(|e| e.into_inner()) = Some(why);
        Ok(false)
    }
}

/// How the reason of a connection refused for a changed host key starts.
const CHANGED: &str = "the host key of";

/// Whether a failure's reason is a host key that changed: trying again ends the same way
/// until the known hosts are put right.
pub fn key_changed(reason: &str) -> bool {
    reason.starts_with(CHANGED)
}

impl Handler for Checker {
    type Error = russh::Error;

    /// A connection to a remote forward's port, opened back by the server.
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        _address: &str,
        port: u32,
        _from: &str,
        _from_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        match crate::forward::asked(port, &self.remotes) {
            Some(to) => {
                reply.accept().await;
                crate::forward::back(channel, to);
            }
            None => reply.reject(ChannelOpenFailure::AdministrativelyProhibited).await,
        }
        Ok(())
    }

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return self.refuse("the host gave a certificate, which OneTerm does not check yet".into());
        };
        let shown =
            if self.port == 22 { self.host.clone() } else { format!("[{}]:{}", self.host, self.port) };
        let print = key.fingerprint(HashAlg::Sha256).to_string();
        let kind = key.algorithm().to_string();
        match check_known_hosts_path(&self.host, self.port, key, &self.file) {
            Ok(true) => Ok(true),
            Ok(false) => {
                let ask = Ask::HostKey { host: shown, kind, fingerprint: print };
                if self.prompt.ask(ask).await.is_none() {
                    return self.refuse(CANCELLED.into());
                }
                if let Err(e) = learn_known_hosts_path(&self.host, self.port, key, &self.file) {
                    self.prompt.say(&format!("[could not add the key to {}: {e}]\n", self.file.display()));
                }
                Ok(true)
            }
            Err(russh::keys::Error::KeyChanged { line }) => self.refuse(format!(
                "{CHANGED} '{shown}' has changed: someone could be eavesdropping on you right now. The {kind} key sent is {print}; the key known is on line {line} of {}. Remove it there to trust the new one.",
                self.file.display()
            )),
            Err(e) => self.refuse(format!("could not read {}: {e}", self.file.display())),
        }
    }
}
