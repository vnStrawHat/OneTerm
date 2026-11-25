//! Offline update-flow tests driven through a fake [`ReleaseClient`] (TEST-04).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use oneterm_core::{AppError, Result};

use super::*;
use crate::github::{GitHubAsset, ReleaseResponse};

const CURRENT: &str = "0.3.0";
const REPOSITORY: &str = "owner/repo";
const ETAG: &str = "\"etag-1\"";

/// Scripted release source: serves one release list (or `304 Not Modified`
/// when the request carries the known ETag) and one asset body.
#[derive(Default)]
struct FakeClient {
    releases: Vec<GitHubRelease>,
    asset_bytes: Vec<u8>,
    /// Requested `If-None-Match` values, newest last.
    requested_etags: Arc<Mutex<Vec<Option<String>>>>,
}

impl ReleaseClient for FakeClient {
    fn fetch_releases(&self, repository: &str, etag: Option<&str>) -> Result<ReleaseResponse> {
        assert_eq!(repository, REPOSITORY);
        self.requested_etags
            .lock()
            .unwrap()
            .push(etag.map(ToOwned::to_owned));
        if etag == Some(ETAG) {
            return Ok(ReleaseResponse {
                etag: None,
                releases: None,
            });
        }
        Ok(ReleaseResponse {
            etag: Some(ETAG.to_owned()),
            releases: Some(self.releases.clone()),
        })
    }

    fn download_to_file(&self, url: &str, path: &Path, max_bytes: u64) -> Result<()> {
        assert!(url.starts_with("https://"), "manager must enforce https");
        if self.asset_bytes.len() as u64 > max_bytes {
            return Err(AppError::msg(
                "update download exceeds the expected size (fake)",
            ));
        }
        std::fs::File::create(path)?.write_all(&self.asset_bytes)?;
        Ok(())
    }
}

/// Isolated cache directory and config document per test.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = test_dir(name);
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn storage(&self) -> UpdateStorage {
        UpdateStorage {
            cache_dir: self.root.join("cache"),
            config_path: self.root.join("update_config.json"),
        }
    }

    fn manager(&self, config: UpdateConfig, client: FakeClient) -> UpdateManager {
        self.manager_for_version(config, client, CURRENT)
    }

    fn manager_for_version(
        &self,
        config: UpdateConfig,
        client: FakeClient,
        current_version: &str,
    ) -> UpdateManager {
        UpdateManager::new(
            REPOSITORY.to_owned(),
            config,
            Box::new(client),
            current_version,
            self.storage(),
        )
    }

    fn persisted_config(&self) -> UpdateConfig {
        UpdateConfig::read_from(&self.root.join("update_config.json")).config
    }

    fn cache_entries(&self) -> Vec<PathBuf> {
        match std::fs::read_dir(self.root.join("cache")) {
            Ok(entries) => entries.map(|entry| entry.unwrap().path()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn release(tag: &str, prerelease: bool, assets: Vec<GitHubAsset>) -> GitHubRelease {
    GitHubRelease {
        tag_name: tag.to_owned(),
        name: None,
        draft: false,
        prerelease,
        body: None,
        html_url: format!("https://github.com/{REPOSITORY}/releases/tag/{tag}"),
        assets,
    }
}

fn asset(version: &str, url: &str, digest: Option<String>) -> GitHubAsset {
    GitHubAsset {
        name: expected_asset_name(version, &current_target_triple()),
        browser_download_url: url.to_owned(),
        size: None,
        digest,
    }
}

fn valid_digest() -> Option<String> {
    Some(format!("sha256:{}", "a".repeat(64)))
}

fn https_asset(version: &str) -> GitHubAsset {
    asset(
        version,
        &format!("https://example.invalid/oneterm-{version}.zip"),
        valid_digest(),
    )
}

fn cached(version: &str, prerelease: bool) -> CachedUpdateCandidate {
    let target = current_target_triple();
    CachedUpdateCandidate {
        version: version.to_owned(),
        tag_name: format!("v{version}"),
        release_name: None,
        release_notes_url: format!("https://github.com/{REPOSITORY}/releases/tag/v{version}"),
        body: None,
        asset_name: expected_asset_name(version, &target),
        asset_url: "https://example.invalid/oneterm.zip".to_owned(),
        asset_digest: "sha256:aa".to_owned(),
        asset_size: Some(3),
        target_triple: target,
        prerelease,
    }
}

/// Config whose cache says "checked `CURRENT` with `ETAG`", so a `Reuse`
/// check sends the ETag and receives `304`.
fn config_with_cache(channel: UpdateChannel, candidate: CachedUpdateCandidate) -> UpdateConfig {
    UpdateConfig {
        channel,
        last_etag: Some(ETAG.to_owned()),
        last_checked_version: Some(CURRENT.to_owned()),
        cached_candidate: Some(candidate),
        ..Default::default()
    }
}

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
fn invalid_build_version_disables_updates_instead_of_offering_everything() {
    let sandbox = Sandbox::new("bad-version");
    let client = FakeClient {
        releases: vec![release("v999.0.0", false, vec![https_asset("999.0.0")])],
        ..Default::default()
    };
    let mut manager = sandbox.manager_for_version(UpdateConfig::default(), client, "not-semver");

    match manager.check_now().unwrap() {
        UpdateCheckResult::Disabled(reason) => assert!(reason.contains("not valid SemVer")),
        other => panic!("expected Disabled, got {other:?}"),
    }
}

#[test]
fn newer_release_with_current_target_asset_is_offered_and_cached() {
    let sandbox = Sandbox::new("offer");
    let client = FakeClient {
        releases: vec![
            release("v0.2.0", false, vec![https_asset("0.2.0")]),
            release("v0.4.0", false, vec![https_asset("0.4.0")]),
            release("v0.5.0-beta.1", true, vec![https_asset("0.5.0-beta.1")]),
        ],
        ..Default::default()
    };
    let mut manager = sandbox.manager(UpdateConfig::default(), client);

    let candidate = match manager.check_now().unwrap() {
        UpdateCheckResult::Available(candidate) => candidate,
        other => panic!("expected Available, got {other:?}"),
    };

    assert_eq!(candidate.version, "0.4.0");
    assert!(!candidate.prerelease);
    let persisted = sandbox.persisted_config();
    assert_eq!(persisted.last_etag.as_deref(), Some(ETAG));
    assert_eq!(persisted.last_checked_version.as_deref(), Some(CURRENT));
    assert_eq!(
        persisted
            .cached_candidate
            .as_ref()
            .map(|c| c.version.as_str()),
        Some("0.4.0")
    );
}

#[test]
fn prerelease_is_offered_only_on_preview_channel() {
    let sandbox = Sandbox::new("prerelease");
    let releases = vec![release(
        "v0.4.0-rc.1",
        true,
        vec![https_asset("0.4.0-rc.1")],
    )];

    let stable = FakeClient {
        releases: releases.clone(),
        ..Default::default()
    };
    let mut manager = sandbox.manager(UpdateConfig::default(), stable);
    assert!(matches!(
        manager.check_now().unwrap(),
        UpdateCheckResult::UpToDate { .. }
    ));

    let preview = FakeClient {
        releases,
        ..Default::default()
    };
    let config = UpdateConfig {
        channel: UpdateChannel::Preview,
        ..Default::default()
    };
    let mut manager = sandbox.manager(config, preview);
    match manager.check_now().unwrap() {
        UpdateCheckResult::Available(candidate) => {
            assert_eq!(candidate.version, "0.4.0-rc.1");
            assert!(candidate.prerelease);
        }
        other => panic!("expected Available, got {other:?}"),
    }
}

#[test]
fn skipped_version_is_not_offered_but_a_newer_one_is() {
    let sandbox = Sandbox::new("skipped");
    let config = UpdateConfig {
        skipped_version: Some("0.4.0".to_owned()),
        ..Default::default()
    };
    let client = FakeClient {
        releases: vec![release("v0.4.0", false, vec![https_asset("0.4.0")])],
        ..Default::default()
    };
    let mut manager = sandbox.manager(config.clone(), client);
    assert!(matches!(
        manager.check_now().unwrap(),
        UpdateCheckResult::UpToDate { .. }
    ));

    let client = FakeClient {
        releases: vec![
            release("v0.4.0", false, vec![https_asset("0.4.0")]),
            release("v0.4.1", false, vec![https_asset("0.4.1")]),
        ],
        ..Default::default()
    };
    let mut manager = sandbox.manager(config, client);
    match manager.check_now().unwrap() {
        UpdateCheckResult::Available(candidate) => assert_eq!(candidate.version, "0.4.1"),
        other => panic!("expected Available, got {other:?}"),
    }
}

#[test]
fn newer_release_without_current_target_is_not_up_to_date() {
    let sandbox = Sandbox::new("no-target");
    let client = FakeClient {
        releases: vec![release(
            "v999.0.0",
            false,
            vec![GitHubAsset {
                name: "oneterm-999.0.0-unsupported-target.zip".to_owned(),
                browser_download_url: "https://example.invalid/oneterm.zip".to_owned(),
                size: None,
                digest: None,
            }],
        )],
        ..Default::default()
    };
    let mut manager = sandbox.manager(UpdateConfig::default(), client);

    assert!(matches!(
        manager.check_now().unwrap(),
        UpdateCheckResult::Disabled(_)
    ));
}

#[test]
fn asset_without_https_url_is_treated_as_missing_package() {
    let sandbox = Sandbox::new("http-asset");
    let client = FakeClient {
        releases: vec![release(
            "v0.4.0",
            false,
            vec![asset(
                "0.4.0",
                "http://example.invalid/oneterm-0.4.0.zip",
                valid_digest(),
            )],
        )],
        ..Default::default()
    };
    let mut manager = sandbox.manager(UpdateConfig::default(), client);

    match manager.check_now().unwrap() {
        UpdateCheckResult::Disabled(reason) => assert!(reason.contains("no compatible")),
        other => panic!("expected Disabled, got {other:?}"),
    }
}

#[test]
fn not_modified_restores_cached_candidate() {
    let sandbox = Sandbox::new("cached-restore");
    let client = FakeClient::default();
    let requested = Arc::clone(&client.requested_etags);
    let config = config_with_cache(UpdateChannel::Stable, cached("999.0.0", false));
    let mut manager = sandbox.manager(config, client);

    match manager.check_now().unwrap() {
        UpdateCheckResult::Available(candidate) => {
            assert_eq!(candidate.version, "999.0.0");
            assert_eq!(candidate.asset_size, Some(3));
        }
        other => panic!("expected Available, got {other:?}"),
    }
    assert_eq!(
        requested.lock().unwrap().as_slice(),
        &[Some(ETAG.to_owned())]
    );
}

#[test]
fn not_modified_drops_cached_prerelease_when_channel_is_stable() {
    let sandbox = Sandbox::new("cached-channel");
    let config = config_with_cache(UpdateChannel::Stable, cached("999.0.0-beta.1", true));
    let mut manager = sandbox.manager(config, FakeClient::default());

    assert!(matches!(
        manager.check_now().unwrap(),
        UpdateCheckResult::UpToDate { .. }
    ));
    assert!(sandbox.persisted_config().cached_candidate.is_none());

    let config = config_with_cache(UpdateChannel::Preview, cached("999.0.0-beta.1", true));
    let mut manager = sandbox.manager(config, FakeClient::default());
    match manager.check_now().unwrap() {
        UpdateCheckResult::Available(candidate) => {
            assert_eq!(candidate.version, "999.0.0-beta.1");
            assert!(candidate.prerelease);
        }
        other => panic!("expected Available, got {other:?}"),
    }
}

#[test]
fn not_modified_drops_cached_candidate_that_was_skipped_since() {
    let sandbox = Sandbox::new("cached-skipped");
    let mut config = config_with_cache(UpdateChannel::Stable, cached("999.0.0", false));
    config.skipped_version = Some("999.0.0".to_owned());
    let mut manager = sandbox.manager(config, FakeClient::default());

    assert!(matches!(
        manager.check_now().unwrap(),
        UpdateCheckResult::UpToDate { .. }
    ));
}

#[test]
fn manual_refresh_bypasses_cached_etag() {
    let sandbox = Sandbox::new("refresh");
    let client = FakeClient {
        releases: vec![release("v0.4.0", false, vec![https_asset("0.4.0")])],
        ..Default::default()
    };
    let requested = Arc::clone(&client.requested_etags);
    let config = config_with_cache(UpdateChannel::Stable, cached("999.0.0", false));
    let mut manager = sandbox.manager(config, client);

    match manager.refresh_now().unwrap() {
        UpdateCheckResult::Available(candidate) => assert_eq!(candidate.version, "0.4.0"),
        other => panic!("expected Available, got {other:?}"),
    }
    assert_eq!(requested.lock().unwrap().as_slice(), &[None]);
}

// ── download_and_stage ───────────────────────────────────────────────

/// Build a release package archive for the current target with the required
/// entry in a versioned package directory. Returns the archive bytes.
fn package_archive_bytes(version: &str) -> Vec<u8> {
    let target = current_target_triple();
    let asset_name = expected_asset_name(version, &target);
    let package = format!("oneterm-{version}-{target}");
    let dir = test_dir("package-archive");
    std::fs::create_dir_all(&dir).unwrap();
    let archive = dir.join(&asset_name);

    if asset_name.ends_with(".zip") {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer
            .start_file(format!("{package}/oneterm.exe"), options)
            .unwrap();
        writer.write_all(b"exe").unwrap();
        writer.finish().unwrap();
    } else {
        let encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(&archive).unwrap(),
            flate2::Compression::fast(),
        );
        let mut builder = tar::Builder::new(encoder);
        let entry = if target.contains("apple-darwin") {
            format!("{package}/OneTerm.app/Contents/MacOS/OneTerm")
        } else {
            format!("{package}/oneterm")
        };
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, entry, &b"bin"[..])
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
    }

    let bytes = std::fs::read(&archive).unwrap();
    let _ = std::fs::remove_dir_all(dir);
    bytes
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn candidate_for(version: &str, bytes: &[u8]) -> UpdateCandidate {
    let target = current_target_triple();
    UpdateCandidate {
        version: version.to_owned(),
        tag_name: format!("v{version}"),
        release_name: None,
        release_notes_url: String::new(),
        body: None,
        asset_name: expected_asset_name(version, &target),
        asset_url: "https://example.invalid/asset".to_owned(),
        asset_digest: format!("sha256:{}", sha256_hex(bytes)),
        asset_size: Some(bytes.len() as u64),
        target_triple: target,
        prerelease: false,
    }
}

#[test]
fn download_and_stage_verifies_digest_extracts_and_validates_package() {
    let sandbox = Sandbox::new("stage-ok");
    let bytes = package_archive_bytes("0.4.0");
    let client = FakeClient {
        asset_bytes: bytes.clone(),
        ..Default::default()
    };
    let manager = sandbox.manager(UpdateConfig::default(), client);
    let candidate = candidate_for("0.4.0", &bytes);

    let staged = manager.download_and_stage(&candidate).unwrap();

    assert_eq!(staged.version, "0.4.0");
    assert!(staged.staging_dir.starts_with(sandbox.root.join("cache")));
    assert!(staged.package_dir.starts_with(&staged.staging_dir));
    assert!(staged.package_dir.is_dir());
    assert!(
        staged
            .package_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("oneterm-0.4.0-")
    );
}

#[test]
fn download_and_stage_rejects_checksum_mismatch_and_cleans_staging() {
    let sandbox = Sandbox::new("stage-digest");
    let bytes = package_archive_bytes("0.4.0");
    let client = FakeClient {
        asset_bytes: bytes.clone(),
        ..Default::default()
    };
    let manager = sandbox.manager(UpdateConfig::default(), client);
    let mut candidate = candidate_for("0.4.0", &bytes);
    candidate.asset_digest = format!("sha256:{}", "b".repeat(64));

    let error = manager
        .download_and_stage(&candidate)
        .expect_err("digest mismatch must fail")
        .to_string();

    assert!(error.contains("checksum mismatch"), "{error}");
    assert!(sandbox.cache_entries().is_empty());
}

#[test]
fn download_and_stage_rejects_body_larger_than_published_size() {
    let sandbox = Sandbox::new("stage-size");
    let bytes = package_archive_bytes("0.4.0");
    let client = FakeClient {
        asset_bytes: bytes.clone(),
        ..Default::default()
    };
    let manager = sandbox.manager(UpdateConfig::default(), client);
    let mut candidate = candidate_for("0.4.0", &bytes);
    candidate.asset_size = Some(bytes.len() as u64 - 1);

    let error = manager
        .download_and_stage(&candidate)
        .expect_err("oversized body must fail")
        .to_string();

    assert!(error.contains("exceeds"), "{error}");
    assert!(sandbox.cache_entries().is_empty());
}

#[test]
fn download_and_stage_refuses_non_https_asset_before_touching_disk() {
    let sandbox = Sandbox::new("stage-http");
    let manager = sandbox.manager(UpdateConfig::default(), FakeClient::default());
    let mut candidate = candidate_for("0.4.0", b"irrelevant");
    candidate.asset_url = "http://example.invalid/asset".to_owned();

    let error = manager
        .download_and_stage(&candidate)
        .expect_err("http asset must fail")
        .to_string();

    assert!(error.contains("non-HTTPS"), "{error}");
    assert!(sandbox.cache_entries().is_empty());
}

fn test_dir(name: &str) -> PathBuf {
    // A process-wide sequence keeps directories distinct even when parallel
    // tests read the same coarse timestamp (as on macOS).
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "oneterm-update-manager-{name}-{}-{nonce}-{sequence}",
        std::process::id()
    ))
}
