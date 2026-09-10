//! Non-secret metadata used to duplicate a live terminal session.

use std::path::PathBuf;

use crate::LocalShellConfig;

/// Authentication fields that an SSH duplicate dialog may prefill.
///
/// This type deliberately excludes passwords and private-key passphrases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshDuplicateAuth {
    /// No authentication material is required.
    None,
    /// Prompt for a password.
    Password,
    /// Prompt for an optional passphrase while retaining the non-secret key path.
    PrivateKey { key_path: PathBuf },
    /// Ask the local SSH agent again; nothing to prompt for.
    Agent,
}

/// Non-secret metadata of one jump host, prompted for again on duplicate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshDuplicateHop {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshDuplicateAuth,
}

/// Non-secret SSH connection metadata retained by a terminal view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshDuplicateConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshDuplicateAuth,
    pub shell_integration: bool,
    /// Jump hosts from the client outwards; empty for a direct connection.
    pub jump_hops: Vec<SshDuplicateHop>,
    /// Port forwards of the source session; the duplicate tries to bind them too
    /// and warns about the ones already taken (DEC-0011).
    pub port_forwards: Vec<crate::PortForward>,
    /// Whether the source session forwarded the local SSH agent.
    pub agent_forwarding: bool,
}

/// Launch metadata required to create a fresh session of the same kind.
#[derive(Clone, Debug)]
pub enum SessionDuplicateConfig {
    Local(LocalShellConfig),
    Ssh(SshDuplicateConfig),
}
