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
    assert_eq!(redact("mysql -p S3cr3t!"), "mysql -p");
}

/// SEC-09 / CORR-49: `-p` is a password only for commands that define it so;
/// the attached form (`-pSECRET`, `-uuser:pass`) keeps the flag, drops the value.
#[test]
fn short_secret_flags_are_per_command_and_detect_attached_values() {
    assert_eq!(redact("mysql -pSECRET db"), "mysql -p db");
    assert_eq!(redact("mysql -p SECRET db"), "mysql -p db");
    assert_eq!(redact("mysql -p=SECRET db"), "mysql -p db");
    assert_eq!(redact("sudo mysql -pSECRET db"), "sudo mysql -p db");
    assert_eq!(
        redact(r"C:\Tools\MySQL.EXE -pSECRET db"),
        r"C:\Tools\MySQL.EXE -p db"
    );
    assert_eq!(redact("curl -uadmin:pw https://x"), "curl -u https://x");
    assert_eq!(
        redact("curl --user admin:pw https://x"),
        "curl --user https://x"
    );
    assert_eq!(redact("docker login -p secret"), "docker login -p");
    assert_eq!(
        redact("docker login -psecret registry"),
        "docker login -p registry"
    );
    assert!(contains_secret("mysql -pSECRET db"));
}

#[test]
fn ordinary_p_flags_keep_their_argument() {
    assert_eq!(redact("mkdir -p dir"), "mkdir -p dir");
    assert_eq!(
        redact("docker run -p 8080:80 img"),
        "docker run -p 8080:80 img"
    );
    assert_eq!(redact("ssh -p 22 host"), "ssh -p 22 host");
    assert_eq!(redact("mysql -u root db"), "mysql -u root db");
    assert!(!contains_secret("mkdir -p dir"));
}

#[test]
fn compound_secret_long_flags_drop_their_value() {
    assert_eq!(
        redact("wget --http-password=x url"),
        "wget --http-password url"
    );
    assert_eq!(
        redact("cli --access-token abc def"),
        "cli --access-token def"
    );
    // A key path is not a secret value.
    assert_eq!(redact("scp -i ~/.ssh/key f h:"), "scp -i ~/.ssh/key f h:");
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
