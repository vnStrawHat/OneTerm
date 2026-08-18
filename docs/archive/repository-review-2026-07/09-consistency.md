# Consistency review — 5.5/10

## What is working

- Package names, crate ownership, and dependency direction are consistent and checked against a machine-readable policy.
- Error and logging libraries are generally used consistently (`thiserror`/`AppError` in libraries, `log`/`env_logger`, no `println!` in product runtime code).
- Settings/session/dock persistence consistently uses shared atomic-write primitives and explicit-path tests.
- Public items are generally documented, and naming follows Rust conventions.

## Findings

### CONS-01 — Medium: the English-only checker does not scan the release scripts

**Files/modules:** `scripts/check-english.py:14-23`, `scripts/build-release.sh:1-18`, `scripts/build-release.ps1:1-19`, `AGENTS.md:41-49`.

**Explanation:** The checker scopes `scripts/` but only accepts suffixes `.md`, `.py`, `.rs`, `.toml`, `.yml`, and `.yaml`. The tracked `.sh` and `.ps1` release scripts contain non-English comments/messages. The checker still reports success.

**Why it matters:** A zero-exception repository rule is contradicted by release-critical files, and automation gives false confidence.

**Recommended solution:** Add `.sh` and `.ps1` to `SUFFIXES`, extract comments for both syntaxes or conservatively scan the whole script, translate existing contributor-facing text, and add a fixture/self-test proving each supported suffix is checked.

### CONS-02 — High: SSH agent authentication is advertised but explicitly unimplemented

**Files/modules:** `README.md:39-44`, `AGENTS.md:185-192`, `crates/core/src/ssh_config.rs:82-101`, `crates/ssh/src/session.rs:253-257`.

**Explanation:** README lists SSH agent authentication; the roadmap marks it complete; the domain enum exposes `Agent`. The backend returns `"SSH agent auth not supported yet (roadmap)"`.

**Why it matters:** Users and maintainers receive incompatible statements about a security-sensitive capability. Tests and UI may assume the enum variant works because it is public.

**Recommended solution:** Either implement agent authentication end-to-end with platform tests, cancellation, key selection, and logging hygiene, or remove/hide the variant from public flows and mark docs as not implemented. Do not leave a public “supported” variant that only fails at runtime.

### CONS-03 — Medium: error policy and implementation diverge on ignored results

**Files/modules:** `docs/agents/error-policy.md:17-28`, `crates/workspace/src/layout/workspace/mod.rs:56-58`, `:311-318`, `crates/workspace/src/layout/workspace/layout.rs:60-65`, `crates/ssh/src/sftp_transfer.rs` cleanup paths.

**Explanation:** The policy requires nearby justification for ignored runtime results and observable failures. Several layout persistence calls use `_ = save_state(...)` without logging or user-visible recovery. Cleanup failures are often intentionally best effort, but not all are annotated/logged consistently.

**Why it matters:** Layout/settings changes may fail silently despite persistence being user-visible state.

**Recommended solution:** Return/log persistence failures with the trigger and path. For cleanup, use a small helper such as `best_effort_cleanup(operation, result)` to make intent and diagnostics consistent.

### CONS-04 — Low: documented line-size policy is routinely violated

**Files/modules:** `docs/agents/structure.md:141-145`, 25 Rust files over 400 lines.

**Explanation:** The rule says to split immediately, but large modules are common in core runtime paths.

**Why it matters:** Inconsistent governance makes code review subjective and trains contributors to ignore other mandatory rules.

**Recommended solution:** Reframe the threshold as a smell with an allowlist/reason, or enforce it. Align policy with actual engineering intent.
