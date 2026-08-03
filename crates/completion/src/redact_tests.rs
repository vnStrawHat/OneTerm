//! Unit tests for the redaction pipeline.

use super::*;

#[test]
fn drops_value_after_secret_flag() {
    assert_eq!(redact("az login --password S3cr3t!"), "az login --password");
}

#[test]
fn drops_inline_secret_value_forms() {
    assert_eq!(redact("login --password=S3cr3t!"), "login --password");
    assert_eq!(redact("login /PASSWORD:S3cr3t!"), "login /PASSWORD");
    assert_eq!(redact("login -p S3cr3t!"), "login -p");
}

#[test]
fn drops_secret_env_assignment() {
    assert_eq!(
        redact("AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI aws s3 ls"),
        "aws s3 ls"
    );
}

#[test]
fn keeps_ordinary_assignment() {
    assert_eq!(
        redact("RUST_LOG=debug cargo test"),
        "RUST_LOG=debug cargo test"
    );
}

#[test]
fn drops_standalone_credential_tokens() {
    assert!(!redact("echo ghp_0123456789abcdefghij").contains("ghp_"));
    assert!(!redact("echo sk-0123456789abcdef0123").contains("sk-"));
    assert!(!redact("echo AKIAIOSFODNN7EXAMPLE").contains("AKIA"));
    let jwt = "eyJhbGciOi.eyJzdWIiOi.SflKxwRJSM";
    assert!(!redact(&format!("echo {jwt}")).contains("eyJ"));
}

#[test]
fn strips_url_userinfo_but_keeps_host() {
    assert_eq!(
        redact("psql postgres://user:pass@db.example.com/app"),
        "psql postgres://db.example.com/app"
    );
}

#[test]
fn does_not_over_redact_normal_command() {
    assert_eq!(redact("dir /Q"), "dir /Q");
    assert_eq!(
        redact("grep --color -n foo file.txt"),
        "grep --color -n foo file.txt"
    );
    assert_eq!(
        redact("cargo build --release --output file.txt"),
        "cargo build --release --output file.txt"
    );
}

#[test]
fn header_bearer_value_dropped_flag_kept() {
    // `curl -H "Authorization: Bearer abc.def"` → the quoted header is one
    // token here (already unquoted by the caller in practice).
    let redacted = redact("curl -H Authorization:Bearer-abcdefgh");
    assert!(redacted.starts_with("curl -H"));
    assert!(!redacted.contains("abcdefgh"));
}

#[test]
fn suggestion_time_guard_detects_injected_secret() {
    assert!(contains_secret("deploy --token ghp_0123456789abcdefghij"));
    assert!(contains_secret("echo AKIAIOSFODNN7EXAMPLE"));
    assert!(!contains_secret("git commit -m message"));
    assert!(!contains_secret("dir /Q"));
}
