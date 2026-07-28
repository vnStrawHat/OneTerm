use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../VERSION");
    println!("cargo:rerun-if-env-changed=ONETERM_UPDATE_REPO");

    let repo_root =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("../..");

    let version = std::fs::read_to_string(repo_root.join("VERSION"))
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|_| "0.0.0".to_owned());
    println!("cargo:rustc-env=ONETERM_VERSION={version}");

    let configured = std::env::var("ONETERM_UPDATE_REPO")
        .ok()
        .and_then(|value| normalize_repo(&value));
    let inferred = configured.or_else(|| infer_git_remote(&repo_root));
    println!(
        "cargo:rustc-env=ONETERM_UPDATE_REPO={}",
        inferred.unwrap_or_default()
    );
}

fn infer_git_remote(repo_root: &std::path::Path) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8(output.stdout).ok()?;
    normalize_repo(raw.trim())
}

fn normalize_repo(raw: &str) -> Option<String> {
    let value = raw.trim().trim_end_matches(".git");
    if value.is_empty() {
        return None;
    }

    if let Some(rest) = value.strip_prefix("https://github.com/") {
        return normalize_owner_repo(rest);
    }
    if let Some(rest) = value.strip_prefix("http://github.com/") {
        return normalize_owner_repo(rest);
    }
    if let Some(rest) = value.strip_prefix("git@github.com:") {
        return normalize_owner_repo(rest);
    }
    if let Some(rest) = value.strip_prefix("ssh://git@github.com/") {
        return normalize_owner_repo(rest);
    }
    normalize_owner_repo(value)
}

fn normalize_owner_repo(value: &str) -> Option<String> {
    let mut parts = value.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}
