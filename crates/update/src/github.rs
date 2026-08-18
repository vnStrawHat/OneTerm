use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

use oneterm_core::{AppError, Result};
use reqwest::Proxy;
use reqwest::blocking::Client;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Whole-request budget for the small releases JSON document.
const RELEASE_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
/// TCP/TLS connect budget for every request.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest pause between two body chunks before a transfer is abandoned.
///
/// The blocking client applies this to the header wait and to every body
/// `read`, so a slow but progressing multi-megabyte download is never cut off
/// by a total deadline (CORR-40).
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Hard cap for a release archive when GitHub reports no asset size (SEC-20).
pub const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;

/// Minimal GitHub Release model needed by the updater.
#[derive(Clone, Debug, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub body: Option<String>,
    pub html_url: String,
    pub assets: Vec<GitHubAsset>,
}

/// Minimal GitHub Release asset model needed by the updater.
#[derive(Clone, Debug, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: Option<u64>,
    pub digest: Option<String>,
}

/// GitHub releases response, including cache metadata.
pub struct ReleaseResponse {
    pub etag: Option<String>,
    pub releases: Option<Vec<GitHubRelease>>,
}

/// Release-source transport used by [`crate::UpdateManager`].
///
/// [`GitHubClient`] is the production implementation; tests substitute an
/// offline double so the check, cache, download, and staging flow can be
/// exercised without network access (TEST-04).
pub trait ReleaseClient: Send {
    /// Fetch the release list, honouring `If-None-Match` when `etag` is given.
    fn fetch_releases(&self, repository: &str, etag: Option<&str>) -> Result<ReleaseResponse>;

    /// Download `url` into `path`, failing once more than `max_bytes` arrive.
    fn download_to_file(&self, url: &str, path: &Path, max_bytes: u64) -> Result<()>;
}

/// Blocking GitHub HTTP client. Callers must run it off the UI thread.
pub struct GitHubClient {
    user_agent: String,
    proxy_url: Option<String>,
    verify_certificates: bool,
}

impl GitHubClient {
    pub fn new(user_agent: String, proxy_url: Option<String>, verify_certificates: bool) -> Self {
        Self {
            user_agent,
            proxy_url: proxy_url.and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            }),
            verify_certificates,
        }
    }

    fn client(&self) -> Result<Client> {
        let mut builder = Client::builder()
            .user_agent(self.user_agent.clone())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(READ_IDLE_TIMEOUT);
        if let Some(proxy_url) = &self.proxy_url {
            let proxy = Proxy::all(proxy_url).map_err(|error| {
                // The proxy URL may embed credentials; never echo them (SEC-19).
                AppError::msg(format!(
                    "invalid update proxy URL '{}': {error}",
                    redact_url_userinfo(proxy_url)
                ))
            })?;
            builder = builder.proxy(proxy);
        }
        if !self.verify_certificates {
            log::warn!(
                "TLS certificate verification for update requests is DISABLED in Settings; \
                 anyone on the network path can serve a forged release list, archive, and \
                 matching SHA-256 digest. Re-enable it outside a trusted network."
            );
            builder = builder.danger_accept_invalid_certs(true);
        }
        builder.build().map_err(http_error)
    }
}

impl ReleaseClient for GitHubClient {
    fn fetch_releases(&self, repository: &str, etag: Option<&str>) -> Result<ReleaseResponse> {
        let url = format!("https://api.github.com/repos/{repository}/releases");
        let client = self.client()?;
        let mut request = client.get(url).timeout(RELEASE_CHECK_TIMEOUT);
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }

        let response = request.send().map_err(http_error)?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(ReleaseResponse {
                etag: None,
                releases: None,
            });
        }
        if !response.status().is_success() {
            return Err(AppError::msg(format!(
                "GitHub update check failed with status {}",
                response.status()
            )));
        }

        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let releases = response.json::<Vec<GitHubRelease>>().map_err(http_error)?;
        Ok(ReleaseResponse {
            etag,
            releases: Some(releases),
        })
    }

    fn download_to_file(&self, url: &str, path: &Path, max_bytes: u64) -> Result<()> {
        require_https(url)?;
        let mut response = self.client()?.get(url).send().map_err(http_error)?;
        if !response.status().is_success() {
            return Err(AppError::msg(format!(
                "download failed with status {}",
                response.status()
            )));
        }
        if let Some(length) = response.content_length()
            && length > max_bytes
        {
            return Err(download_too_large(length, max_bytes));
        }
        let mut file = File::create(path)?;
        let mut buffer = [0_u8; 64 * 1024];
        let mut received: u64 = 0;
        loop {
            let read = response.read(&mut buffer).map_err(std::io::Error::other)?;
            if read == 0 {
                break;
            }
            received = received.saturating_add(read as u64);
            if received > max_bytes {
                return Err(download_too_large(received, max_bytes));
            }
            file.write_all(&buffer[..read])?;
        }
        file.flush()?;
        Ok(())
    }
}

/// Only HTTPS release assets are ever fetched (SEC-22).
pub fn require_https(url: &str) -> Result<()> {
    if is_https_url(url) {
        Ok(())
    } else {
        Err(AppError::msg(format!(
            "refusing to download update asset over a non-HTTPS URL: {url}"
        )))
    }
}

pub fn is_https_url(url: &str) -> bool {
    url.get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
}

fn download_too_large(received: u64, max_bytes: u64) -> AppError {
    AppError::msg(format!(
        "update download exceeds the expected size ({received} bytes received, limit {max_bytes} bytes)"
    ))
}

/// Replace the `user:password@` part of a URL with `***@` for log and error text.
pub fn redact_url_userinfo(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_owned();
    };
    let authority_start = scheme_end + 3;
    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |offset| authority_start + offset);
    let authority = &url[authority_start..authority_end];
    let Some(at) = authority.rfind('@') else {
        return url.to_owned();
    };
    format!(
        "{}***@{}",
        &url[..authority_start],
        &url[authority_start + at + 1..]
    )
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verify_asset_digest(asset_name: &str, expected_digest: &str, actual: &str) -> Result<()> {
    let expected = expected_digest
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or(expected_digest.trim());
    if expected.len() != 64 || !expected.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AppError::msg(format!(
            "GitHub Release asset {asset_name} has an invalid SHA-256 digest: {expected_digest}",
        )));
    }
    if expected.eq_ignore_ascii_case(actual) {
        return Ok(());
    }
    Err(AppError::msg(format!(
        "checksum mismatch for {asset_name}: expected {expected}, got {actual}",
    )))
}

fn http_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() {
        AppError::msg(format!("HTTP timeout: {error}"))
    } else {
        AppError::msg(format!("HTTP error: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_digest_accepts_sha256_prefix() {
        let digest = format!("sha256:{}", "a".repeat(64));
        verify_asset_digest("oneterm.zip", &digest, &"a".repeat(64)).unwrap();
    }

    #[test]
    fn asset_digest_mismatch_is_rejected() {
        let expected = format!("sha256:{}", "a".repeat(64));
        assert!(verify_asset_digest("oneterm.zip", &expected, &"b".repeat(64)).is_err());
    }

    #[test]
    fn proxy_userinfo_is_redacted_but_host_and_path_survive() {
        assert_eq!(
            redact_url_userinfo("http://alice:s3cret@proxy.example:8080/path?x=1"),
            "http://***@proxy.example:8080/path?x=1"
        );
        assert_eq!(
            redact_url_userinfo("socks5://user@proxy.example"),
            "socks5://***@proxy.example"
        );
    }

    #[test]
    fn urls_without_userinfo_are_unchanged() {
        assert_eq!(
            redact_url_userinfo("http://proxy.example:8080"),
            "http://proxy.example:8080"
        );
        assert_eq!(redact_url_userinfo("not a url"), "not a url");
        assert_eq!(
            redact_url_userinfo("http://proxy.example/a@b"),
            "http://proxy.example/a@b"
        );
    }

    #[test]
    fn only_https_asset_urls_are_accepted() {
        assert!(require_https("https://github.com/o/r/releases/download/v1/a.zip").is_ok());
        assert!(require_https("HTTPS://github.com/a.zip").is_ok());
        assert!(require_https("http://github.com/a.zip").is_err());
        assert!(require_https("file:///tmp/a.zip").is_err());
        assert!(require_https("").is_err());
    }
}
