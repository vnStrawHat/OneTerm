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

const RELEASE_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const DOWNLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(60);

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

    pub fn fetch_releases(&self, repository: &str, etag: Option<&str>) -> Result<ReleaseResponse> {
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

    pub fn download_to_file(&self, url: &str, path: &Path) -> Result<()> {
        let mut response = self
            .client()?
            .get(url)
            .timeout(DOWNLOAD_TOTAL_TIMEOUT)
            .send()
            .map_err(http_error)?;
        if !response.status().is_success() {
            return Err(AppError::msg(format!(
                "download failed with status {}",
                response.status()
            )));
        }
        let mut file = File::create(path)?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = response.read(&mut buffer).map_err(std::io::Error::other)?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])?;
        }
        file.flush()?;
        Ok(())
    }

    fn client(&self) -> Result<Client> {
        let mut builder = Client::builder().user_agent(self.user_agent.clone());
        if let Some(proxy_url) = &self.proxy_url {
            let proxy = Proxy::all(proxy_url).map_err(|error| {
                AppError::msg(format!("invalid update proxy URL '{proxy_url}': {error}"))
            })?;
            builder = builder.proxy(proxy);
        }
        if !self.verify_certificates {
            builder = builder.danger_accept_invalid_certs(true);
        }
        builder.build().map_err(http_error)
    }
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
}
