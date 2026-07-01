//! russh client handler — minimal for MVP.
//!
//! Roadmap: known_hosts + prompt accept/reject (see design doc §9.3).

use russh::client;

/// Minimal handler — MVP accepts every host key (NOT production-safe).
pub(crate) struct SshClientHandler;

impl client::Handler for SshClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        // MVP: accept every host key. TODO: known_hosts verification.
        Ok(true)
    }
}
