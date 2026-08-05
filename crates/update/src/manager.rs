use std::path::PathBuf;

use oneterm_core::Result;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::archive::{extract_archive, validate_staged_package};
use crate::config::{CachedUpdateCandidate, UpdateChannel, UpdateConfig};
use crate::github::{self, GitHubClient, GitHubRelease};
use crate::version::{current_target_triple, expected_asset_name, parse_release_version};
use crate::{CURRENT_VERSION, UPDATE_REPOSITORY};

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
    current_version: Version,
    client: GitHubClient,
    config: UpdateConfig,
}

impl UpdateManager {
    /// Build a manager from persisted config and the build-time GitHub repository.
    pub fn load() -> Self {
        Self::with_repository(UPDATE_REPOSITORY.to_owned(), UpdateConfig::load())
    }

    /// Build a manager with an explicit repository, used by tests and custom callers.
    pub fn with_repository(repository: String, config: UpdateConfig) -> Self {
        let current_version =
            parse_release_version(CURRENT_VERSION).unwrap_or_else(|_| Version::new(0, 0, 0));
        Self {
            repository,
            current_version,
            client: GitHubClient::new(
                format!("OneTerm/{CURRENT_VERSION}"),
                config.proxy_url.clone(),
                config.verify_certificates,
            ),
            config,
        }
    }

    /// Return the persisted config snapshot.
    pub fn config(&self) -> &UpdateConfig {
        &self.config
    }

    /// Check whether the automatic interval permits a background check now.
    pub fn should_auto_check(&self) -> bool {
        self.config.should_auto_check()
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

        let current_version = self.current_version.to_string();
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
                UpdateCheckResult::UpToDate {
                    current_version: self.current_version.to_string(),
                }
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
        let version = parse_release_version(&cached.version).ok()?;
        if version <= self.current_version {
            return None;
        }
        if self.config.skipped_version.as_deref() == Some(cached.version.as_str()) {
            return None;
        }
        Some(Box::new(cached.into()))
    }

    fn persist_config(&self) {
        if let Err(error) = self.config.save() {
            log::warn!("failed to persist update_config.json after check: {error}");
        }
    }

    /// Download, verify, extract, and validate a candidate update.
    pub fn download_and_stage(&self, candidate: &UpdateCandidate) -> Result<StagedUpdate> {
        let stage_root = update_cache_dir().join(format!(
            "{}-{}-{}",
            candidate.version,
            candidate.target_triple,
            chrono::Utc::now().timestamp_millis()
        ));
        std::fs::create_dir_all(&stage_root)?;

        let result = (|| -> Result<StagedUpdate> {
            let artifact_path = stage_root.join(&candidate.asset_name);
            self.client
                .download_to_file(&candidate.asset_url, &artifact_path)?;

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
            let _ = std::fs::remove_dir_all(&stage_root);
        }

        result
    }

    fn select_candidate(&self, releases: &[GitHubRelease]) -> Result<CandidateSelection> {
        let target = current_target_triple();
        log::info!(
            "Evaluating GitHub releases for current version {} and target {}.",
            self.current_version,
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
            if version <= self.current_version {
                log::info!(
                    "Skipping GitHub Release {} because it is not newer than current version {}.",
                    release.tag_name,
                    self.current_version
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
                    self.current_version
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
        })))
    }
}

/// Directory for transient update downloads and staged packages.
fn update_cache_dir() -> PathBuf {
    oneterm_core::config_dir().join("updates")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::GitHubAsset;

    #[test]
    fn expected_asset_matches_current_target() {
        let target = current_target_triple();
        let asset = expected_asset_name("0.3.0", &target);
        assert!(asset.starts_with("oneterm-0.3.0-"));
        assert!(asset.contains(&target));
    }

    #[test]
    fn disabled_when_repository_is_empty() {
        let mut manager = UpdateManager::with_repository(String::new(), UpdateConfig::default());
        assert!(matches!(
            manager.check_now().unwrap(),
            UpdateCheckResult::Disabled(_)
        ));
    }
    #[test]
    fn newer_release_without_current_target_is_not_up_to_date() {
        let manager =
            UpdateManager::with_repository("owner/repo".to_owned(), UpdateConfig::default());
        let release = GitHubRelease {
            tag_name: "v999.0.0".to_owned(),
            name: None,
            draft: false,
            prerelease: false,
            body: None,
            html_url: "https://github.com/owner/repo/releases/tag/v999.0.0".to_owned(),
            assets: vec![GitHubAsset {
                name: "oneterm-999.0.0-unsupported-target.zip".to_owned(),
                browser_download_url: "https://example.invalid/oneterm.zip".to_owned(),
                size: None,
                digest: None,
            }],
        };

        assert!(matches!(
            manager.select_candidate(&[release]).unwrap(),
            CandidateSelection::NoCompatiblePackage
        ));
    }

    #[test]
    fn newer_release_with_versioned_current_target_asset_is_available() {
        let manager =
            UpdateManager::with_repository("owner/repo".to_owned(), UpdateConfig::default());
        let target = current_target_triple();
        let release = GitHubRelease {
            tag_name: "v999.0.0".to_owned(),
            name: None,
            draft: false,
            prerelease: false,
            body: None,
            html_url: "https://github.com/owner/repo/releases/tag/v999.0.0".to_owned(),
            assets: vec![GitHubAsset {
                name: expected_asset_name("999.0.0", &target),
                browser_download_url: "https://example.invalid/oneterm.zip".to_owned(),
                size: None,
                digest: Some(format!("sha256:{}", "a".repeat(64))),
            }],
        };

        match manager.select_candidate(&[release]).unwrap() {
            CandidateSelection::Candidate(candidate) => {
                assert_eq!(candidate.version, "999.0.0");
                assert_eq!(candidate.asset_digest, format!("sha256:{}", "a".repeat(64)));
                assert_eq!(candidate.target_triple, target);
            }
            other => panic!("expected candidate, got {other:?}"),
        }
    }
    #[test]
    fn cached_candidate_restores_current_target_update() {
        let target = current_target_triple();
        let mut config = UpdateConfig::default();
        config.cached_candidate = Some(CachedUpdateCandidate {
            version: "999.0.0".to_owned(),
            tag_name: "v999.0.0".to_owned(),
            release_name: Some("OneTerm 999.0.0".to_owned()),
            release_notes_url: "https://github.com/owner/repo/releases/tag/v999.0.0".to_owned(),
            body: None,
            asset_name: expected_asset_name("999.0.0", &target),
            asset_url: "https://example.invalid/oneterm.zip".to_owned(),
            asset_digest: format!("sha256:{}", "a".repeat(64)),
            asset_size: Some(123),
            target_triple: target.clone(),
        });
        let manager = UpdateManager::with_repository("owner/repo".to_owned(), config);

        let candidate = manager.cached_candidate().expect("cached candidate");

        assert_eq!(candidate.version, "999.0.0");
        assert_eq!(candidate.target_triple, target);
        assert_eq!(candidate.asset_size, Some(123));
    }
}
