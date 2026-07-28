use oneterm_core::{AppError, Result};
use semver::Version;

pub fn parse_release_version(tag: &str) -> Result<Version> {
    let normalized = tag.trim().trim_start_matches('v');
    Version::parse(normalized)
        .map_err(|error| AppError::msg(format!("invalid release version '{tag}': {error}")))
}

pub fn current_target_triple() -> String {
    format!("{}-{}", current_arch(), current_os_env())
}

pub fn expected_asset_name(version: &str, target: &str) -> String {
    let normalized_version = version.trim().trim_start_matches('v');
    if target.contains("windows") {
        format!("oneterm-{normalized_version}-{target}.zip")
    } else {
        format!("oneterm-{normalized_version}-{target}.tar.gz")
    }
}

fn current_arch() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64"
    }
    #[cfg(all(not(target_arch = "x86_64"), not(target_arch = "aarch64")))]
    {
        std::env::consts::ARCH
    }
}

fn current_os_env() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "pc-windows-msvc"
    }
    #[cfg(target_os = "linux")]
    {
        "unknown-linux-gnu"
    }
    #[cfg(target_os = "macos")]
    {
        "apple-darwin"
    }
    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "linux"),
        not(target_os = "macos")
    ))]
    {
        std::env::consts::OS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v_prefixed_semver() {
        assert_eq!(
            parse_release_version("v1.2.3").unwrap(),
            Version::new(1, 2, 3)
        );
    }

    #[test]
    fn builds_versioned_asset_name() {
        assert_eq!(
            expected_asset_name("0.3.0", "x86_64-pc-windows-msvc"),
            "oneterm-0.3.0-x86_64-pc-windows-msvc.zip"
        );
        assert_eq!(
            expected_asset_name("0.3.0", "x86_64-unknown-linux-gnu"),
            "oneterm-0.3.0-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn rejects_invalid_semver() {
        assert!(parse_release_version("release-1").is_err());
    }
}
