//! SSH connection config — re-exported from `oneterm-core`.
//!
//! The types moved to the leaf `oneterm-core` crate so the UI feature crates can
//! build an `SshConfig` without depending on this backend crate. This shim keeps
//! `crate::config::{SshConfig, SshAuthMethod}` and `oneterm_ssh::config` working.

pub use oneterm_core::{SshAuthMethod, SshConfig};
