use std::path::PathBuf;

use oneterm_core::Result;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::CURRENT_VERSION;
use crate::archive::{extract_archive, validate_staged_package};
use crate::config::{CachedUpdateCandidate, UpdateChannel, UpdateCheckCache, UpdateConfig};
use crate::github::{self, GitHubClient, GitHubRelease, ReleaseClient};
use crate::version::{current_target_triple, expected_asset_name, parse_release_version};

/// A release artifact that can update this build.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub version: String,
    pub tag_name: String,
    pub release_name: Option<String>,
    pub release_notes_url: String,
    pub body: Option<String>,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_digest: String,
    pub asset_size: Option<u64>,
    pub target_triple: String,
    /// Whether the release is a GitHub prerelease (offered on `preview` only).
    #[serde(default)]
    pub prerelease: bool,
}

impl From<&UpdateCandidate> for CachedUpdateCandidate {
    fn from(candidate: &UpdateCandidate) -> Self {
        Self {
            version: candidate.version.clone(),
            tag_name: candidate.tag_name.clone(),
            release_name: candidate.release_name.clone(),
            release_notes_url: candidate.release_notes_url.clone(),
            body: candidate.body.clone(),
            asset_name: candidate.asset_name.clone(),
            asset_url: candidate.asset_url.clone(),
            asset_digest: candidate.asset_digest.clone(),
            asset_size: candidate.asset_size,
            target_triple: candidate.target_triple.clone(),
            prerelease: candidate.prerelease,
        }
    }
}

impl From<CachedUpdateCandidate> for UpdateCandidate {
    fn from(candidate: CachedUpdateCandidate) -> Self {
        Self {
            version: candidate.version,
            tag_name: candidate.tag_name,
            release_name: candidate.release_name,
            release_notes_url: candidate.release_notes_url,
            body: candidate.body,
            asset_name: candidate.asset_name,
            asset_url: candidate.asset_url,
            asset_digest: candidate.asset_digest,
            asset_size: candidate.asset_size,
            target_triple: candidate.target_triple,
            prerelease: candidate.prerelease,
        }
    }
}

/// Result of checking GitHub Releases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheckResult {
    /// Updating is disabled or impossible; the string explains why.
    Disabled(String),
    /// The current build is already the newest compatible release.
    UpToDate { current_version: String },
    /// A newer compatible release is available to download.
    Available(Box<UpdateCandidate>),
}

#[derive(Debug)]
enum CandidateSelection {
    Candidate(Box<UpdateCandidate>),
    NoCompatiblePackage,
    None,
}

/// Whether a release check may reuse the cached ETag or must fetch fresh data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EtagCachePolicy {
    /// Reuse the stored ETag when it still matches the current app version.
    Reuse,
    /// Ignore any cached ETag and force a fresh GitHub request.
    Bypass,
}

/// A verified update staged on disk and ready for platform installation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedUpdate {
    pub version: String,
    pub staging_dir: PathBuf,
    pub package_dir: PathBuf,
    pub target_triple: String,
}

/// GitHub Releases updater.
pub struct UpdateManager {
    repository: String,
    /// `None` when the build's own version is not valid SemVer; every check
    /// then reports [`UpdateCheckResult::Disabled`] instead of treating each
    /// release as newer than `0.0.0` (SEC-22).
    current_version: Option<Version>,
    client: Box<dyn ReleaseClient>,
    config: UpdateConfig,
    storage: UpdateStorage,
}

/// On-disk locations the manager writes to.
pub(crate) struct UpdateStorage {
    /// Root for downloads, staged packages, and the Windows install backup.
    pub(crate) cache_dir: PathBuf,
    /// The `update_config.json` document that receives the check cache.
    pub(crate) config_path: PathBuf,
}

impl Default for UpdateStorage {
    fn default() -> Self {
        Self {
            cache_dir: update_cache_dir(),
            config_path: crate::config::update_config_path(),
        }
    }
}

impl UpdateManager {
    /// Build a manager with an explicit repository, used by tests and custom callers.
    pub fn with_repository(repository: String, config: UpdateConfig) -> Self {
        let client = GitHubClient::new(
            format!("OneTerm/{CURRENT_VERSION}"),
            config.proxy_url.clone(),
            config.verify_certificates,
        );
        Self::new(
            repository,
            config,
            Box::new(client),
            CURRENT_VERSION,
            UpdateStorage::default(),
        )
    }

    fn new(
        repository: String,
        config: UpdateConfig,
        client: Box<dyn ReleaseClient>,
        current_version: &str,
        storage: UpdateStorage,
    ) -> Self {
        let current_version = match parse_release_version(current_version) {
            Ok(version) => Some(version),
            Err(error) => {
                log::error!(
                    "the running build's version is not valid SemVer; automatic updates are disabled: {error}"
                );
                None
            }
        };
        Self {
            repository,
            current_version,
            client,
            config,
            storage,
        }
    }

    /// Cache metadata recorded by the last check. Callers that hold the live
    /// preferences must merge only this into their config.
    pub fn check_cache(&self) -> UpdateCheckCache {
        self.config.check_cache()
    }

    /// Check GitHub Releases and persist cache metadata after a successful request.
    pub fn check_now(&mut self) -> Result<UpdateCheckResult> {
        self.check_with_cache(EtagCachePolicy::Reuse)
    }

    /// Force a fresh GitHub Releases request without reusing the cached ETag.
    pub fn refresh_now(&mut self) -> Result<UpdateCheckResult> {
        self.check_with_cache(EtagCachePolicy::Bypass)
    }

    fn check_with_cache(&mut self, cache_policy: EtagCachePolicy) -> Result<UpdateCheckResult> {
        if self.repository.trim().is_empty() {
            return Ok(UpdateCheckResult::Disabled(
                "No GitHub release repository is configured.".to_owned(),
            ));
        }
        let Some(current_version) = self.current_version.clone() else {
            return Ok(UpdateCheckResult::Disabled(format!(
                "This build reports version '{CURRENT_VERSION}', which is not valid SemVer; updates are disabled."
            )));
        };

        let current_version = current_version.to_string();
        let reuse_cached_etag = cache_policy == EtagCachePolicy::Reuse
            && self.config.should_reuse_cached_etag(&current_version);
        match cache_policy {
            EtagCachePolicy::Reuse => {
                if !reuse_cached_etag && self.config.last_etag.is_some() {
                    log::info!(
                        "Current app version changed since the last update check; ignoring cached ETag and refreshing GitHub releases."
                    );
                }
            }
            EtagCachePolicy::Bypass => {
                if self.config.last_etag.is_some() {
                    log::info!(
                        "Manual update check is bypassing the cached ETag and refreshing GitHub releases."
                    );
                }
            }
        }

        let response = self.client.fetch_releases(
            &self.repository,
            if reuse_cached_etag {
                self.config.last_etag.as_deref()
            } else {
                None
            },
        )?;
        self.config
            .record_success(response.etag.clone(), &current_version);

        let Some(releases) = response.releases else {
            let cached_candidate = if cache_policy == EtagCachePolicy::Reuse {
                self.cached_candidate()
            } else {
                None
            };
            if cached_candidate.is_none() {
                self.config.cached_candidate = None;
            }
            self.persist_config();
            if let Some(candidate) = cached_candidate {
                log::info!(
                    "GitHub releases are unchanged; restoring cached update candidate {}.",
                    candidate.version
                );
                return Ok(UpdateCheckResult::Available(candidate));
            }
            return Ok(UpdateCheckResult::UpToDate { current_version });
        };

        let result = match self.select_candidate(&releases)? {
            CandidateSelection::Candidate(candidate) => {
                self.config.cached_candidate =
                    Some(CachedUpdateCandidate::from(candidate.as_ref()));
                UpdateCheckResult::Available(candidate)
            }
            CandidateSelection::NoCompatiblePackage => {
                self.config.cached_candidate = None;
                UpdateCheckResult::Disabled(
                    "GitHub has a newer release, but no compatible update package is available for this platform.".to_owned(),
                )
            }
            CandidateSelection::None => {
                self.config.cached_candidate = None;
                UpdateCheckResult::UpToDate { current_version }
            }
        };
        self.persist_config();
        Ok(result)
    }

    fn cached_candidate(&self) -> Option<Box<UpdateCandidate>> {
        let cached = self.config.cached_candidate.clone()?;
        if cached.target_triple != current_target_triple() {
            return None;
        }
        // The cache was filled under the channel active at that time; the
        // user may have switched back to stable since (CORR-37).
        if cached.prerelease && self.config.channel != UpdateChannel::Preview {
            return None;
        }
        let version = parse_release_version(&cached.version).ok()?;
        if Some(&version) <= self.current_version.as_ref() {
            return None;
        }
        if self.config.skipped_version.as_deref() == Some(cached.version.as_str()) {
            return None;
        }
        Some(Box::new(cached.into()))
    }

    /// Persist only the checker-owned cache fields. Preferences are owned by
    /// the settings UI and may have changed while this check was running, so
    /// the manager never writes the whole document.
    fn persist_config(&self) {
        if let Err(error) = self.config.check_cache().save_to(&self.storage.config_path) {
            log::warn!("failed to persist update_config.json cache after check: {error}");
        }
    }

    /// Download, verify, extract, and validate a candidate update.
    pub fn download_and_stage(&self, candidate: &UpdateCandidate) -> Result<StagedUpdate> {
        github::require_https(&candidate.asset_url)?;
        let stage_root = self.storage.cache_dir.join(format!(
            "{}-{}-{}",
            candidate.version,
            candidate.target_triple,
            chrono::Utc::now().timestamp_millis()
        ));
        std::fs::create_dir_all(&stage_root)?;

        let result = (|| -> Result<StagedUpdate> {
            let artifact_path = stage_root.join(&candidate.asset_name);
            // GitHub publishes the exact asset size; anything beyond it (or the
            // hard cap when the size is unknown) is aborted mid-stream (SEC-20).
            let max_bytes = candidate
                .asset_size
                .filter(|size| *size > 0)
                .unwrap_or(github::MAX_DOWNLOAD_BYTES);
            self.client
                .download_to_file(&candidate.asset_url, &artifact_path, max_bytes)?;

            let actual = github::sha256_file(&artifact_path)?;
            github::verify_asset_digest(&candidate.asset_name, &candidate.asset_digest, &actual)?;

            let extract_dir = stage_root.join("extracted");
            std::fs::create_dir_all(&extract_dir)?;
            extract_archive(&artifact_path, &extract_dir)?;
            let package_dir = validate_staged_package(&extract_dir, &candidate.target_triple)?;

            Ok(StagedUpdate {
                version: candidate.version.clone(),
                staging_dir: stage_root.clone(),
                package_dir,
                target_triple: candidate.target_triple.clone(),
            })
        })();

        if result.is_err() {
            // Best effort: the download failure is what gets reported; a
            // leftover staging directory only wastes disk space, but say so.
            if let Err(error) = std::fs::remove_dir_all(&stage_root) {
                log::warn!(
                    "failed to remove update staging directory {}: {error}",
                    stage_root.display()
                );
            }
        }

        result
    }

    fn select_candidate(&self, releases: &[GitHubRelease]) -> Result<CandidateSelection> {
        let Some(current_version) = self.current_version.as_ref() else {
            return Ok(CandidateSelection::None);
        };
        let target = current_target_triple();
        log::info!(
            "Evaluating GitHub releases for current version {} and target {}.",
            current_version,
            target
        );
        let mut candidates = Vec::new();
        let mut newer_release_missing_package = false;

        for release in releases {
            if release.draft {
                log::info!("Skipping draft GitHub Release {}.", release.tag_name);
                continue;
            }
            if release.prerelease && self.config.channel != UpdateChannel::Preview {
                log::info!(
                    "Skipping prerelease GitHub Release {} because update channel is stable.",
                    release.tag_name
                );
                continue;
            }
            let Ok(version) = parse_release_version(&release.tag_name) else {
                log::warn!(
                    "ignoring GitHub Release with non-SemVer tag: {}",
                    release.tag_name
                );
                continue;
            };
            if &version <= current_version {
                log::info!(
                    "Skipping GitHub Release {} because it is not newer than current version {}.",
                    release.tag_name,
                    current_version
                );
                continue;
            }
            let version_string = version.to_string();
            if self.config.skipped_version.as_deref() == Some(version_string.as_str()) {
                log::info!(
                    "Skipping GitHub Release {} because version {} was skipped in settings.",
                    release.tag_name,
                    version
                );
                continue;
            }

            let expected_asset = expected_asset_name(&version_string, &target);
            let Some(asset) = release
                .assets
                .iter()
                .find(|asset| asset.name == expected_asset)
            else {
                newer_release_missing_package = true;
                log::warn!(
                    "GitHub Release {} is newer, but missing the expected asset {}.",
                    release.tag_name,
                    expected_asset
                );
                continue;
            };
            let Some(digest) = asset
                .digest
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            else {
                newer_release_missing_package = true;
                log::warn!(
                    "GitHub Release {} is newer, but missing the asset digest for {}.",
                    release.tag_name,
                    asset.name
                );
                continue;
            };
            if !github::is_https_url(&asset.browser_download_url) {
                newer_release_missing_package = true;
                log::warn!(
                    "GitHub Release {} asset {} is not served over HTTPS; ignoring it.",
                    release.tag_name,
                    asset.name
                );
                continue;
            }

            log::info!(
                "GitHub Release {} matches the current platform package {}.",
                release.tag_name,
                asset.name
            );
            candidates.push((version, release, asset, digest));
        }

        candidates.sort_by(|left, right| right.0.cmp(&left.0));
        let Some((version, release, asset, digest)) = candidates.into_iter().next() else {
            if newer_release_missing_package {
                log::warn!(
                    "GitHub has newer releases, but none matched the current platform target {}.",
                    target
                );
            } else {
                log::info!(
                    "No GitHub release is newer than the current version {}.",
                    current_version
                );
            }
            return Ok(if newer_release_missing_package {
                CandidateSelection::NoCompatiblePackage
            } else {
                CandidateSelection::None
            });
        };

        Ok(CandidateSelection::Candidate(Box::new(UpdateCandidate {
            version: version.to_string(),
            tag_name: release.tag_name.clone(),
            release_name: release.name.clone(),
            release_notes_url: release.html_url.clone(),
            body: release.body.clone(),
            asset_name: asset.name.clone(),
            asset_url: asset.browser_download_url.clone(),
            asset_digest: digest.to_owned(),
            asset_size: asset.size,
            target_triple: target,
            prerelease: release.prerelease,
        })))
    }
}

/// Directory for transient update downloads, staged packages, and the
/// Windows helper's install backup.
pub(crate) fn update_cache_dir() -> PathBuf {
    oneterm_core::config_dir().join("updates")
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod manager_tests;
