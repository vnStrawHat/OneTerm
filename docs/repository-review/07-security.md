# Security review — 7.5/10

## Threat model assumed

The terminal processes local and remote-controlled byte streams. Remote SSH servers, remote filenames/metadata, OSC payloads, hyperlinks, clipboard requests, and downloaded file trees are untrusted. The local user controls selected files, saved hosts, and shell configuration. This review assumes the host OS account and installed OneTerm binary are trusted.

## Strong controls found

- **SSH host verification:** strict by default, exact fingerprint approval, changed-key rejection, and persisted trust tests (`crates/ssh/src/handler.rs:144-173`, `:209-370`).
- **Secret handling:** passwords/passphrases use `ZeroizeOnDrop`, masked `Debug`, and no serialization (`crates/core/src/ssh_config.rs:14-45`, `:82-101`).
- **SFTP path safety:** remote components reject separators, traversal, Windows device names, trailing dot/space, and symlinks; directory depth/entry limits bound hostile trees (`crates/ssh/src/sftp_task.rs:378-535`, `crates/ssh/src/sftp_transfer.rs:19-20`).
- **Safe transfer finalization:** downloads/uploads use temporary siblings and do not replace directories/symlinks (`crates/ssh/src/sftp_transfer.rs:62-137`).
- **Terminal-controlled data:** title/notification/CWD length and characters are constrained; notification rates/queues are bounded; remote clipboard operations default off (`crates/terminal/src/security_policy.rs:16-68`, `:106-175`).
- **Paste protection:** embedded bracketed-paste terminators are removed and size is capped (`crates/terminal/src/paste.rs:38-64`).
- **External target policy:** schemes are allowlisted, credential-bearing authorities are denied, and non-default ports/display mismatches require confirmation (`crates/terminal/src/url_policy.rs:62-145`).

## Findings

### SEC-01 — Medium: GitHub Actions are tag-pinned rather than commit-pinned

**Files/modules:** `.github/workflows/ci.yml:18-20`, `:40-42`, `.github/workflows/release.yml:53`, `:122-130`, `:198-223`.

**Explanation:** The workflows use third-party actions such as `actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, and `Swatinem/rust-cache@v2`. Tags are mutable references. The release workflow has `contents: write` and can commit/tag/publish artifacts.

**Why it matters:** Compromise or retargeting of an action reference can execute with repository or release permissions. Release jobs are a high-value software-supply-chain boundary.

**Recommended solution:** Pin every action to a reviewed immutable commit SHA, use Dependabot/Renovate to propose updates, keep job-level permissions minimal, and protect release dispatch through an environment requiring approval. Consider artifact attestations/signing and checksums in release notes.

### SEC-02 — Medium: clipboard security policy is split between the backend and UI

**Files/modules:** `crates/ssh/src/listener.rs:76-96`, `:328-335`, `crates/terminal/src/security_policy.rs:49-67`, `crates/settings/src/terminal_settings/mod.rs:143-147`, `crates/settings-ui/src/terminal_options.rs:324-341`, `crates/terminal-view/src/view/mod.rs:448-466`.

**Explanation:** The SSH listener constructs `TerminalSecurityPolicy::default()`, whose remote clipboard-read flag is false, and refuses to emit `ClipboardRead`. Separately, the UI setting can enable `allow_clipboard_read` and the terminal view checks it before reading the clipboard. No production code transfers the setting into the SSH listener policy.

**Why it matters:** The current result is fail-closed, but the user-facing setting claims it can enable reads “including remote programs over SSH.” More importantly, two independent decisions make future changes easy to get wrong—for example, enabling only one layer and believing the other protects it.

**Recommended solution:** Construct a per-session immutable `TerminalSecurityPolicy` from validated settings and pass it to the listener. Keep one authoritative backend decision; the UI may retain a defense-in-depth check but should not define separate semantics. Add tests for local/remote × enabled/disabled.

### SEC-03 — Medium: custom URL parsing is not a complete URL validation boundary

**Files/modules:** `crates/terminal/src/url_policy.rs:147-220`.

**Explanation:** The code parses only scheme, a literal `@`, and a trailing numeric segment. It does not fully validate authority, host, percent-encoding, or bracketed IPv6. The scheme allowlist prevents obvious custom-scheme launches, but future policy additions may assume more parser guarantees than exist.

**Why it matters:** URL-opening policy is exposed to remote-controlled terminal content. Security decisions should rely on a well-defined parser and canonical representation.

**Recommended solution:** Use a mature URL parser if dependency policy permits. Otherwise explicitly reject any form outside a narrow grammar and add adversarial tests for malformed authority, bracketed IPv6 with/without port, encoded userinfo, Unicode hostnames, empty host, backslashes, and control/whitespace variants.

### SEC-04 — Low: release binaries are published without a repository-defined signing/attestation step

**Files/modules:** `.github/workflows/release.yml:146-203`, `:219-257`.

**Explanation:** The workflow builds and uploads archives but does not sign binaries/archives or publish provenance/checksums.

**Why it matters:** Users cannot independently verify artifact origin beyond GitHub transport. This is more important for an SSH client that handles credentials and remote sessions.

**Recommended solution:** Add SHA-256 checksum files, GitHub artifact attestations/SLSA provenance, and platform signing where feasible (Authenticode, Apple notarization). Document what is and is not signed.

## Assumptions and non-findings

- No persisted password was found. `ssh_session.json` contains label/host/port/username/group only.
- Logging intentionally records connection identifiers and file paths but tests ensure terminal write payload secrets are not logged (`crates/ssh/src/listener.rs:444-462`). Hostnames/usernames are privacy-sensitive operational data; deployments needing stronger privacy should lower log verbosity.
- The shell-integration command is a fixed constant, not composed from user-controlled host/path strings (`crates/ssh/src/session.rs:466-482`), so no direct command-injection path was identified there.
