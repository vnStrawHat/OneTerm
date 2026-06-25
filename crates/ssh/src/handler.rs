//! russh client handler — tối thiểu cho MVP.
//!
//! Roadmap: known_hosts + prompt accept/reject (xem design doc §9.3).

use russh::client;

/// Handler tối thiểu — MVP chấp nhận mọi host key (KHÔNG an toàn production).
pub(crate) struct SshClientHandler;

impl client::Handler for SshClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        // MVP: chấp nhận mọi host key. TODO: known_hosts verification.
        Ok(true)
    }
}
