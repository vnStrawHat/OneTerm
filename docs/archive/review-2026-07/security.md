# Security Review

**Score: 3.5 / 10**

## SEC-01 — SSH server identity is never verified

- **Files:** `crates/ssh/src/handler.rs:1-19`, `crates/ssh/src/session.rs:120-129`
- **Modules:** `oneterm-ssh::handler`, SSH connection setup
- **Severity:** **Critical / release blocker**
- **Explanation:** `SshClientHandler::check_server_key` unconditionally returns `Ok(true)`. The source itself labels this “NOT production-safe.” No known-hosts lookup, first-use prompt, fingerprint display, pinned key, or changed-key rejection exists.
- **Why it matters:** SSH encryption without server authentication does not prevent an active man-in-the-middle attack. An attacker can impersonate the target, receive passwords/commands, and tamper with terminal/SFTP data.
- **Recommended solution:** Implement OpenSSH-compatible known-hosts verification before allowing production connections. Reject changed keys. For an unknown key, show SHA-256 fingerprint and algorithm in a modal, and persist only after explicit approval. A development override must be explicit, visibly unsafe, default-off, and unavailable in release builds unless deliberately enabled.
- **Example implementation:** Make the handler carry `host`, `port`, a known-hosts repository, and an async decision channel. Return `false` for mismatch; return `true` only for a match or an explicit one-time/persisted approval. Until that exists, fail closed rather than accept all keys.

## SEC-02 — Remote directory entries can escape the local download root

- **Files:** `crates/ssh/src/sftp_task.rs:741-769,786-815`
- **Modules:** SFTP recursive download
- **Severity:** **High**
- **Explanation:** The recursive walker filters only names exactly equal to `.` or `..`, then computes `local.join(&name)`. It does not reject separators, rooted paths, Windows prefixes, `../x`, or verify that the normalized path remains beneath the user-selected root.
- **Why it matters:** A malicious/compromised server can attempt to overwrite files outside the selected destination. Cross-platform handling is particularly important because both `/` and `\` may have path meaning on Windows.
- **Recommended solution:** Treat every remote entry name as one path component. Reject empty names, `.`, `..`, separators, absolute/root/prefix components, and names that normalize outside the root. Define an explicit symlink policy. Canonicalize the destination parent where possible and enforce containment before creating files.
- **Example implementation:**

```rust
fn safe_child(root: &Path, remote_name: &str) -> Result<PathBuf> {
    if remote_name.is_empty()
        || remote_name == "."
        || remote_name == ".."
        || remote_name.contains(['/', '\\'])
    {
        return Err(AppError::msg("unsafe remote filename"));
    }
    let child = root.join(remote_name);
    if !child.starts_with(root) {
        return Err(AppError::msg("download path escaped destination"));
    }
    Ok(child)
}
```

`starts_with` is only one layer; production code should also account for symlinks/canonical parents and platform prefixes.

## SEC-03 — Notification limits are declared but not enforced

- **Files:** `crates/terminal/src/security_policy.rs:26-46,53-65,87-100`; `crates/terminal-view/src/view/mod.rs:210-214,471-475`; `crates/terminal-view/src/render/mod.rs:33-38`
- **Modules:** Terminal security policy, terminal event handling
- **Severity:** **Medium**
- **Explanation:** `notification_rate_per_sec` and `max_queued_notifications` are public policy fields with secure-looking defaults, but repository search finds no use outside their declaration/default. Every accepted notification is pushed to an unbounded `Vec`, then all are converted into UI notifications during render.
- **Why it matters:** A remote process can flood the UI with bounded-size but high-rate messages, causing allocation, render pressure, and denial of service. The policy currently creates a false assurance because its documented controls are inert.
- **Recommended solution:** Enforce a token bucket/sliding window in the backend listener or terminal view, and store notifications in a bounded `VecDeque`. Drop/coalesce excess events with one visible summary. Test burst, sustained-rate, and queue-cap behavior.

## SEC-04 — Secrets are ordinary cloneable strings without explicit clearing

- **Files:** `crates/core/src/ssh_config.rs:13-31`; `crates/session-ui/src/connect_dialog.rs:224-251`; `crates/session-ui/src/quick_connect_dialog.rs:42-62,125-132`; `crates/ssh/src/session.rs:133-158`
- **Modules:** SSH authentication model and dialogs
- **Severity:** **Low/Medium**
- **Explanation:** Passwords and private-key passphrases are held in `String`/`Option<String>` and the config derives `Clone`. Logging correctly masks passwords, and credentials are not serialized, but memory is not zeroized after authentication.
- **Why it matters:** Heap snapshots, crash dumps, or memory-disclosure vulnerabilities can retain credentials longer than necessary. This is defense-in-depth, not a substitute for host verification.
- **Recommended solution:** Use a secret wrapper (`secrecy`/`zeroize`) for credentials, avoid cloning where possible, clear the input state after dispatch, and zeroize temporary credentials immediately after auth completes. Keep the existing masked `Debug` behavior.

## SEC-05 — Release automation is not supply-chain hardened

- **Files:** `.github/workflows/release.yml:53,122-130,198-223`
- **Modules:** GitHub Actions release workflow
- **Severity:** **Medium**
- **Explanation:** Third-party actions are referenced by mutable major tags (`actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, upload/download actions) rather than immutable commit SHAs. The workflow also grants write permissions to preparation/release jobs.
- **Why it matters:** A compromised action tag or upstream account can affect release artifacts. This workflow publishes executable SSH-client binaries, so provenance matters.
- **Recommended solution:** Pin every action to a reviewed commit SHA, minimize job-scoped permissions, generate checksums/SBOM, sign artifacts or attest provenance, and add dependency auditing.

## Security strengths

- `SshAuthMethod` has a masked custom `Debug` implementation and credentials are not persisted (`crates/core/src/ssh_config.rs:54-82`).
- Terminal titles, cwd, notifications, control characters, and BiDi controls pass through a central policy (`crates/terminal/src/security_policy.rs`).
- Remote OSC clipboard reads/writes default to disabled (`security_policy.rs:53-65,102-124`).
- URL policy rejects embedded credentials and unsafe schemes; confirmation-required links are not opened until UI confirmation exists (`crates/terminal/src/url_policy.rs`, `crates/terminal-view/src/handlers/mouse.rs:42-60`).
- Agent OSC payloads have an 8 KiB envelope cap and display-side field truncation (`crates/terminal/src/osc_agent/mod.rs:48-55`, `crates/state/src/agent_registry.rs:30-69`).
