//! SSH agent authentication (IN-0023 / US-0057).
//!
//! Finds the local SSH agent and offers its identities to the server, in agent
//! order, until one is accepted. No key material enters OneTerm: the agent
//! signs, the client only relays public keys. Discovery order follows OpenSSH:
//! the `openssh-ssh-agent` named pipe then Pageant on Windows, `SSH_AUTH_SOCK`
//! on Unix. See `docs/ssh-authentication.md`.

use russh::MethodKind;
use russh::client::{AuthResult, Handle};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};

use oneterm_core::{AppError, ConnectPhase};

use crate::handler::SshClientHandler;
use crate::session::{authentication_failure_message, phase_error, rsa_hash_alg};

/// Identities offered per connection. OpenSSH's default `MaxAuthTries` is 6;
/// beyond that the server drops the connection mid-loop, which would surface as
/// a transport error instead of a clear "none accepted".
pub(crate) const MAX_AGENT_IDENTITIES: usize = 6;

/// The agent connection, type-erased so one code path serves every transport.
pub(crate) type BoxedAgentStream = Box<dyn AgentStream + Send + Unpin>;

#[cfg(windows)]
const OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

/// Connect to the local SSH agent.
///
/// Every candidate that fails is named in the error so a missing agent is
/// diagnosable from the notification alone.
pub(crate) async fn connect_agent() -> oneterm_core::Result<AgentClient<BoxedAgentStream>> {
    let stream = connect_agent_stream().await?;
    Ok(AgentClient::connect(stream))
}

#[cfg(windows)]
pub(crate) async fn connect_agent_stream() -> oneterm_core::Result<BoxedAgentStream> {
    let pipe_error = match AgentClient::connect_named_pipe(OPENSSH_AGENT_PIPE).await {
        Ok(agent) => return Ok(agent.into_inner()),
        Err(error) => error,
    };
    let pageant_error = match AgentClient::connect_pageant().await {
        Ok(agent) => return Ok(agent.into_inner()),
        Err(error) => error,
    };
    Err(no_agent_error(&[
        (OPENSSH_AGENT_PIPE, pipe_error.to_string()),
        ("Pageant", pageant_error.to_string()),
    ]))
}

#[cfg(unix)]
pub(crate) async fn connect_agent_stream() -> oneterm_core::Result<BoxedAgentStream> {
    match AgentClient::connect_env().await {
        Ok(agent) => Ok(agent.into_inner()),
        Err(error) => Err(no_agent_error(&[("$SSH_AUTH_SOCK", error.to_string())])),
    }
}

fn no_agent_error(tried: &[(&str, String)]) -> AppError {
    let tried: Vec<String> = tried
        .iter()
        .map(|(name, error)| format!("{name}: {error}"))
        .collect();
    phase_error(
        ConnectPhase::Authentication,
        format!("no SSH agent is reachable ({})", tried.join("; ")),
    )
}

/// Authenticate `user` with the identities of the local SSH agent.
pub(crate) async fn authenticate_with_agent(
    handle: &mut Handle<SshClientHandler>,
    user: &str,
) -> oneterm_core::Result<AuthResult> {
    let mut agent = connect_agent().await?;
    authenticate_with_agent_client(handle, user, &mut agent).await
}

/// Offer the agent's identities one by one until the server accepts one.
///
/// Stops early when the server no longer lists `publickey` among the methods
/// it accepts, and after [`MAX_AGENT_IDENTITIES`] attempts. Every rejection is
/// logged; the final error names the methods the server still accepts and how
/// many identities were offered.
pub(crate) async fn authenticate_with_agent_client<S>(
    handle: &mut Handle<SshClientHandler>,
    user: &str,
    agent: &mut AgentClient<S>,
) -> oneterm_core::Result<AuthResult>
where
    S: AgentStream + Send + Unpin,
{
    let auth_error =
        |error: &dyn std::fmt::Display| phase_error(ConnectPhase::Authentication, error);
    let identities = agent
        .request_identities()
        .await
        .map_err(|error| auth_error(&format!("SSH agent did not list its identities: {error}")))?;
    if identities.is_empty() {
        return Err(auth_error(
            &"the SSH agent holds no identities (add one with ssh-add)",
        ));
    }
    log::info!(
        "SshSession: the SSH agent holds {} identities; offering up to {MAX_AGENT_IDENTITIES}",
        identities.len()
    );

    // Queried once: the answer does not change within a connection.
    let mut rsa_hash = None;
    let mut last_failure: Option<(russh::MethodSet, bool)> = None;
    let mut offered = 0usize;
    for identity in identities.iter().take(MAX_AGENT_IDENTITIES) {
        let public_key = identity.public_key().into_owned();
        let hash_alg = if public_key.algorithm().is_rsa() {
            match rsa_hash {
                Some(hash) => hash,
                None => {
                    let advertised = handle
                        .best_supported_rsa_hash()
                        .await
                        .map_err(|error| auth_error(&error))?;
                    let hash = rsa_hash_alg(advertised);
                    rsa_hash = Some(hash);
                    hash
                }
            }
        } else {
            None
        };
        offered += 1;
        log::info!(
            "SshSession: offering agent identity {offered} ({}{})",
            public_key.algorithm(),
            identity_comment(identity)
        );
        match handle
            .authenticate_publickey_with(user, public_key, hash_alg, agent)
            .await
            .map_err(|error| auth_error(&error))?
        {
            AuthResult::Success => return Ok(AuthResult::Success),
            AuthResult::Failure {
                remaining_methods,
                partial_success,
            } => {
                log::info!("SshSession: agent identity {offered} rejected");
                let publickey_still_accepted = remaining_methods.contains(&MethodKind::PublicKey);
                last_failure = Some((remaining_methods, partial_success));
                if !publickey_still_accepted {
                    log::info!("SshSession: the server stopped accepting publickey");
                    break;
                }
            }
        }
    }

    let (remaining_methods, partial_success) =
        last_failure.expect("at least one identity is offered before the loop ends");
    Err(auth_error(&format!(
        "{}; none of the {offered} agent identit{} offered was accepted",
        authentication_failure_message(&remaining_methods, partial_success),
        if offered == 1 { "y" } else { "ies" }
    )))
}

fn identity_comment(identity: &AgentIdentity) -> String {
    let comment = identity.comment();
    if comment.is_empty() {
        String::new()
    } else {
        format!(", {comment}")
    }
}

// Loop tests against an in-process agent and server live in a sibling file (see code-style.md).
#[cfg(test)]
#[path = "agent_tests.rs"]
mod agent_tests;
