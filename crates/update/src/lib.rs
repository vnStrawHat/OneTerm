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

pub use config::{
    CachedUpdateCandidate, DEFAULT_UPDATE_REPOSITORY, UPDATE_REPOSITORY, UpdateChannel,
    UpdateCheckCache, UpdateConfig,
};
pub use install::{InstallOutcome, install_staged_update};
pub use manager::{StagedUpdate, UpdateCandidate, UpdateCheckResult, UpdateManager};

/// Current app version (the workspace `version` in the root `Cargo.toml`).
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
