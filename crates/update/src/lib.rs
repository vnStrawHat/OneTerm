//! Auto-update service backed by GitHub Releases.
//!
//! This crate is UI-free. Callers run the blocking operations on a background
//! executor and mirror the returned status into their own GPUI state.

mod archive;
mod config;
mod github;
mod install;
mod manager;
mod version;

pub use config::{CachedUpdateCandidate, UpdateChannel, UpdateCheckCache, UpdateConfig};
pub use install::{InstallOutcome, install_staged_update};
pub use manager::{StagedUpdate, UpdateCandidate, UpdateCheckResult, UpdateManager};

/// Current app version embedded from the repo-root `VERSION` file.
pub const CURRENT_VERSION: &str = env!("ONETERM_VERSION");

/// GitHub `owner/repo` inferred from `remote.origin.url` at build time.
pub const UPDATE_REPOSITORY: &str = env!("ONETERM_UPDATE_REPO");
