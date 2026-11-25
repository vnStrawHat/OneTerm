//! The connection route (IN-0023 / US-0058): TCP to the first hop, a
//! direct-tcpip channel of the previous hop for every later one, and one
//! authentication per hop. `connect` walks `SshConfig::jump_hops` then the
//! target through [`connect_hop`]; the hop handles stay alive in
//! [`JumpHandles`] inside `ssh_main_task` and drop after the target.

use std::borrow::Cow;
use std::sync::Arc;

use russh::client::{self, AuthResult, Handle};
use russh::keys::PrivateKeyWithHashAlg;

use oneterm_core::{AppError, ConnectPhase, SshAuthMethod, SshKeepaliveConfig};

use crate::agent::authenticate_with_agent;
use crate::handler::SshClientHandler;
use crate::session::{
    ConnectPhases, authenticate_with_password, authentication_failure_message, load_private_key,
    phase_error, rsa_hash_alg,
};

/// Authenticated jump-host handles, innermost last. Dropping the set closes
/// the hops from the innermost outwards, so no hop's connection closes under
/// a hop that still rides on it.
#[derive(Default)]
pub(crate) struct JumpHandles(Vec<Handle<SshClientHandler>>);

impl JumpHandles {
    pub(crate) fn push(&mut self, handle: Handle<SshClientHandler>) {
        self.0.push(handle);
    }

    /// The hop the next transport rides on (`None` for a direct TCP connection).
    pub(crate) fn last(&self) -> Option<&Handle<SshClientHandler>> {
        self.0.last()
    }
}

impl Drop for JumpHandles {
    fn drop(&mut self) {
        while let Some(handle) = self.0.pop() {
            drop(handle);
        }
    }
}

/// The russh client config for one hop: keepalive policy plus the host-key
/// algorithm preference derived from `known_hosts`.
pub(crate) fn client_config(
    handler: &SshClientHandler,
    keepalive: SshKeepaliveConfig,
) -> Arc<client::Config> {
    let mut config = client::Config {
        keepalive_interval: keepalive.interval(),
        keepalive_max: keepalive.max(),
        ..Default::default()
    };
    config.preferred.key = Cow::Owned(handler.preferred_key_algorithms());
    Arc::new(config)
}

/// Open the SSH transport to `host:port`: over TCP when `carrier` is `None`,
/// otherwise over a direct-tcpip channel of `carrier`.
pub(crate) async fn open_transport(
    carrier: Option<&Handle<SshClientHandler>>,
    host: &str,
    port: u16,
    handler: SshClientHandler,
    config: Arc<client::Config>,
) -> oneterm_core::Result<Handle<SshClientHandler>> {
    match carrier {
        None => client::connect(config, format!("{host}:{port}"), handler)
            .await
            .map_err(|error| error.to_app_error()),
        Some(carrier) => {
            let channel = carrier
                .channel_open_direct_tcpip(host, u32::from(port), "127.0.0.1", 0)
                .await
                .map_err(|error| {
                    phase_error(
                        ConnectPhase::Transport,
                        format!(
                            "the jump host refused a direct-tcpip channel to {host}:{port}: {error}"
                        ),
                    )
                })?;
            client::connect_stream(config, channel.into_stream(), handler)
                .await
                .map_err(|error| error.to_app_error())
        }
    }
}

/// Authenticate `username` on `handle` with `auth`; a server rejection is an
/// `Authentication` error naming the methods the server still accepts.
pub(crate) async fn authenticate(
    handle: &mut Handle<SshClientHandler>,
    username: &str,
    auth: SshAuthMethod,
) -> oneterm_core::Result<()> {
    let auth_error = |error: russh::Error| phase_error(ConnectPhase::Authentication, error);
    let result = match auth {
        SshAuthMethod::None => {
            log::info!("SshSession: authenticating with none (no password)");
            handle
                .authenticate_none(username)
                .await
                .map_err(auth_error)?
        }
        SshAuthMethod::Password { password } => {
            log::info!("SshSession: authenticating with password");
            authenticate_with_password(handle, username, password.expose_secret()).await?
        }
        SshAuthMethod::PrivateKey {
            key_path,
            passphrase,
        } => {
            log::info!("SshSession: authenticating with key {}", key_path.display());
            // Key files are read and decrypted on the blocking pool so the two
            // SSH runtime workers keep serving other sessions (CORR-17).
            let key = tokio::task::spawn_blocking(move || {
                load_private_key(
                    &key_path,
                    passphrase.as_ref().map(|secret| secret.expose_secret()),
                )
            })
            .await
            .unwrap_or_else(|join_error| {
                Err(phase_error(ConnectPhase::Authentication, join_error))
            })?;
            // RSA keys must not sign with the legacy SHA-1 `ssh-rsa` (OpenSSH
            // >= 8.8 rejects it). Ask the server which `rsa-sha2-*` it
            // supports (RFC 8308 `server-sig-algs`); when it does not say,
            // prefer SHA-512.
            let hash_alg = if key.algorithm().is_rsa() {
                rsa_hash_alg(handle.best_supported_rsa_hash().await.map_err(auth_error)?)
            } else {
                None
            };
            handle
                .authenticate_publickey(
                    username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
                )
                .await
                .map_err(auth_error)?
        }
        SshAuthMethod::Agent => {
            log::info!("SshSession: authenticating with the local SSH agent");
            authenticate_with_agent(handle, username).await?
        }
    };
    log::info!("SshSession: auth result = {result:?}");
    match result {
        AuthResult::Success => Ok(()),
        AuthResult::Failure {
            remaining_methods,
            partial_success,
        } => Err(AppError::Connect {
            phase: ConnectPhase::Authentication,
            message: authentication_failure_message(&remaining_methods, partial_success),
        }),
    }
}

/// Open the transport to the hop (or target) `handler` describes and
/// authenticate it, each step under its connect phase deadline. The caller
/// builds the handler, so only the target's carries forwards and the agent
/// bridge; a hop never serves server-opened channels.
pub(crate) async fn connect_hop(
    phases: &ConnectPhases,
    carrier: Option<&Handle<SshClientHandler>>,
    handler: SshClientHandler,
    username: &str,
    auth: SshAuthMethod,
    keepalive: SshKeepaliveConfig,
) -> oneterm_core::Result<Handle<SshClientHandler>> {
    let host = handler.host().to_string();
    let port = handler.port();
    let config = client_config(&handler, keepalive);
    let mut handle = phases
        .run(
            ConnectPhase::Transport,
            open_transport(carrier, &host, port, handler, config),
        )
        .await?;
    log::info!(
        "SshSession: transport to {host}:{port} open{}",
        if carrier.is_some() {
            " (through the jump host)"
        } else {
            ""
        }
    );
    phases
        .run(
            ConnectPhase::Authentication,
            authenticate(&mut handle, username, auth),
        )
        .await?;
    Ok(handle)
}

/// Attribute a hop failure to its hop. Host-key errors already carry the hop's
/// host and port (the UI picks the hop from them), and a cancellation is not
/// a hop's fault; every other connect error is prefixed with the hop label.
pub(crate) fn hop_error(label: &str, error: AppError) -> AppError {
    match error {
        AppError::Connect { phase, message } => AppError::Connect {
            phase,
            message: format!("jump host {label}: {message}"),
        },
        other => other,
    }
}

// Route tests against two in-process servers live in a sibling file (see code-style.md).
#[cfg(test)]
#[path = "route_tests.rs"]
mod route_tests;
