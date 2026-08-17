# Auto-update specification

## Status

Planned. This document defines the target behavior and implementation boundaries for
OneTerm auto-update. The update source is GitHub Releases.

## Goals

- Let users discover, download, and install newer OneTerm releases from the official
  GitHub repository without visiting the release page manually.
- Keep the update flow explicit and recoverable: never interrupt active terminal,
  SSH, or SFTP sessions without user confirmation.
- Reuse the existing release version source (`VERSION`) and release staging layout
  (`dist/oneterm-<triple>/`).
- Support Windows, Linux, and macOS with platform-specific installation behavior.
- Verify downloaded artifacts before installing them.
- Support locked-down enterprise networks with explicit proxy settings, automatic
  system proxy fallback, and a user-visible certificate verification toggle.

## Non-goals

- Silent forced updates.
- Updating development builds (`oneterm-debug`) by default.
- Package-manager integration for distro packages, Homebrew, winget, or MSI/DMG
  installers in the first implementation.
- Delta or binary patch updates. The first implementation downloads complete release
  artifacts.
- Updating while preserving modified files inside the application install directory.
  User configuration remains in the user config directory and is not part of the
  release artifact.

## Release source

The updater reads releases from the official GitHub repository configured at build
time or in a trusted static constant:

```text
https://api.github.com/repos/<owner>/<repo>/releases
```

Default channel behavior:

- `stable`: use the newest non-draft, non-prerelease release.
- `preview`: include prereleases, but never include drafts.

Version rules:

- Release tags must be SemVer-compatible: `vMAJOR.MINOR.PATCH` or
  `MAJOR.MINOR.PATCH`.
- The current app version is read from the same `VERSION` value used by release
  builds.
- The updater offers an update only when the release version is strictly greater
  than the current version.
- Build metadata does not make a release newer. Pre-release versions are offered
  only on the `preview` channel.

GitHub API requirements:

- Send a deterministic `User-Agent`, for example `OneTerm/<version>`.
- Cache `ETag` and use `If-None-Match` to avoid unnecessary rate-limit usage.
- Reuse a cached `ETag` only when the current app version matches the version used
  for the last successful check; a new build must re-evaluate release availability.
- Treat `304 Not Modified` as a successful check with no new update.
- Network requests must run off the UI thread.
- If an explicit update proxy is configured, use it for GitHub API and asset
  downloads. Otherwise, allow the HTTP stack to use the system or environment
  proxy configuration.
- TLS certificate verification is enabled by default. Disabling it is an
  advanced setting for controlled networks only and must not disable checksum
  verification.
- No GitHub token is required for public releases. If authenticated requests are
  added later, tokens must never be logged or persisted in plaintext.

## Release artifacts

Each GitHub Release must contain one archive per supported target triple. The archive
name includes the release version and target triple, for example
`oneterm-0.3.0-x86_64-pc-windows-msvc.zip` or
`oneterm-0.3.0-x86_64-unknown-linux-gnu.tar.gz`. The archive
contains exactly the staged distribution directory produced by the release scripts.

Required asset names:

| Platform | Target triple | Asset |
|---|---|---|
| Windows x64 | `x86_64-pc-windows-msvc` | `oneterm-<version>-x86_64-pc-windows-msvc.zip` |
| Windows ARM64 | `aarch64-pc-windows-msvc` | `oneterm-<version>-aarch64-pc-windows-msvc.zip` |
| Linux x64 | `x86_64-unknown-linux-gnu` | `oneterm-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | `oneterm-<version>-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | `oneterm-<version>-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `oneterm-<version>-aarch64-apple-darwin.tar.gz` |

Required verification metadata:

- Use each GitHub Release asset's `digest` value (`sha256:<hex>`) returned by
  the GitHub Releases API.
- Signature artifacts are optional until release signing is enabled.

Recommended metadata asset:

```json
{
  "schema_version": 1,
  "version": "0.4.0",
  "minimum_supported_version": "0.3.0",
  "release_notes_url": "https://github.com/<owner>/<repo>/releases/tag/v0.4.0",
  "assets": [
    {
      "target": "x86_64-pc-windows-msvc",
      "name": "oneterm-0.4.0-x86_64-pc-windows-msvc.zip",
      "sha256": "...",
      "size": 12345678
    }
  ]
}
```

The GitHub Release remains the source of truth. The metadata asset is only a bounded,
machine-readable index for the assets attached to that release.

## User experience

Entry points:

- Automatic background check after startup when enabled.
- Manual `Install Update` action from the AppMenuBar ▸ About dialog when an
  update is already available.
- Optional update notification when a newer version is available.

Default settings:

- Auto-check is enabled by default and can be disabled in Settings.
- Auto-check is disabled for debug builds.
- Check interval is 24 hours.
- Channel is `stable`.
- Download and install are confirmed from the About dialog when an update is available.
- Proxy URL is empty by default, which means automatic system/environment proxy
  detection is used.
- GitHub TLS certificate verification is enabled by default.

Required states:

| State | UI behavior |
|---|---|
| Checking | Show non-blocking progress in Settings and the About dialog. |
| Up to date | Show current version and last successful check time. |
| Update available | Show version, release notes link, artifact size, and an `Install Update` action. |
| Downloading | Show progress, downloaded bytes, total bytes when known, and `Cancel`. |
| Installing | Show progress while the update is being downloaded and applied. |
| Failed | Show a corrective message and keep the current installation unchanged. |

Session safety:

- If terminal, SSH, SFTP, or agent work is active, `Install and restart` must warn
  that active sessions will close.
- The updater must never close sessions automatically only because an update was
  downloaded.

## Persistence

Durable update preferences should be owned by `oneterm-settings` in a dedicated
`update_config.json` document, unless implementation chooses to extend `ui_config.json`
with a documented schema migration.

Suggested schema:

```json
{
  "schema_version": 1,
  "auto_check": true,
  "channel": "stable",
  "check_interval_hours": 24,
  "proxy_url": null,
  "verify_certificates": true,
  "last_checked_at": "2026-07-28T00:00:00Z",
  "last_etag": "...",
  "skipped_version": null
}
```

Persistence rules:

- Use the shared atomic persistence helpers from `oneterm-core`.
- Do not perform filesystem writes on the UI thread.
- Invalid persisted config must be quarantined before defaults are written.
- Downloaded artifacts are runtime cache data and should live under a dedicated
  update cache directory, not beside user settings.

Writers and field ownership (`crates/update/src/config.rs`):

| Field group | Fields | Owner / writer | Write path |
|---|---|---|---|
| Preferences | `auto_check`, `channel`, `check_interval_hours`, `proxy_url`, `verify_certificates`, `skipped_version` | `oneterm-settings-ui` (`updates/config.rs` persist queue, `UpdateConfig` entity is the in-memory truth) | `UpdateConfig::save_preferences` |
| Check cache | `last_checked_at`, `last_etag`, `last_checked_version`, `cached_candidate` | `oneterm-update` (`UpdateManager` after a successful GitHub response) | `UpdateCheckCache::save` |

Both write paths are field-level `update_json_file` merges under the shared
inter-process lock, so neither writer can clobber the other's fields even when a
preference is edited while a check is running. Only the initial default document
(created when the file is missing) is written whole. When a check completes, the
UI merges just the returned `UpdateCheckCache` into the live entity
(`UpdateConfig::apply_check_cache`); it never replaces the entity with the
manager's stale pre-check copy.

## Architecture

Recommended crate split:

- `oneterm-core`: shared error variants and small domain types only when required by
  multiple lower layers.
- `oneterm-settings`: persistent update preferences.
- `oneterm-state`: runtime update status and app-level notification integration, if
  the status is shared across windows.
- `oneterm-update` (new shared service crate, to be added only after following the
  new-crate process): GitHub release checking, asset selection, download, checksum
  verification, staging, and platform installer orchestration. This crate must not
  depend on GPUI or feature UI crates.
- `oneterm-settings-ui`: settings/about controls that call the update service through
  an app service handle or command callback.
- `oneterm-app`: composition root that installs the update service and wires platform
  capabilities.

Dependency constraints:

- Feature UI crates must not own update protocol logic.
- The workspace shell must remain feature-agnostic.
- Network and filesystem work must run on background executors.
- The updater must not depend on SSH, SFTP, terminal rendering, or local PTY crates.

## Update check flow

1. Load update preferences.
2. If automatic checks are disabled or the interval has not elapsed, stop.
3. Request releases from GitHub using cached `ETag` when available.
4. Filter drafts, prereleases according to channel, and invalid SemVer tags.
5. Pick the newest compatible release.
6. Select the asset matching the current target triple.
7. If no asset matches, report `No compatible update package for this platform`.
8. Compare release version with current version.
9. Publish `Up to date` or `Update available` runtime status.
10. Persist `last_checked_at` and `last_etag` after a successful check.

## Download and verification flow

1. Create a fresh staging directory under the update cache directory.
2. Download the selected asset to a temporary file.
3. Report progress from content length when available.
4. Compute SHA-256 while downloading or immediately after download.
5. Compare the hash with the selected GitHub Release asset's `digest` metadata.
6. Verify release signature when signing is available.
7. Extract the archive into a staging directory.
8. Validate required files for the current platform:
   - Windows: `oneterm.exe`; `conpty.dll` and `x64/OpenConsole.exe` when present in
     the release artifact.
   - Linux: `oneterm` executable.
   - macOS: `OneTerm.app` bundle.
9. Mark the update as `Ready to install` only after verification succeeds.

If any step fails, delete incomplete temporary files when safe, keep the current app
unchanged, and report a typed user-facing error.

## Installation behavior

Installation must be atomic from the user's perspective: either the next launch uses
the new version or the old version remains available.

Windows:

- The running executable cannot be overwritten in place.
- Spawn a small updater helper or relaunch command after the main process exits.
- Do not require PowerShell. Some environments block PowerShell by policy, so the
  helper path must use a signed helper binary or a minimal `cmd.exe` batch helper.
- Replace the complete distribution directory, including `oneterm.exe`, `conpty.dll`,
  and `x64/OpenConsole.exe`.
- Preserve a rollback copy until the new process starts successfully.

Linux:

- Support portable installs where the application directory is user-writable.
- Replace the staged binary atomically with a rename when possible.
- If the install location is not writable, keep the verified archive and show a
  manual installation action.

macOS:

- Replace the whole `OneTerm.app` bundle, not just the executable inside it.
- If the bundle is under a protected location such as `/Applications`, request user
  confirmation and fall back to manual installation if elevation is unavailable.
- Signed and notarized releases are strongly recommended before enabling automatic
  replacement by default.

Rollback:

- Keep the previous installation until the first launch attempt of the new version.
- If replacement fails, restore the previous installation and show a notification.
- If the new process fails to launch, leave a recovery marker that allows the next
  launch to report the failed update and keep using the previous version when
  possible.

## Security requirements

Minimum for first release:

- Use HTTPS GitHub API and asset URLs only.
- Match assets by exact target triple and expected extension.
- Verify SHA-256 before extraction or installation.
- Reject archives with absolute paths, parent-directory traversal, or symlinks that
  escape the staging directory.
- Do not execute downloaded content before verification.
- Do not log full download URLs if they contain temporary tokens.
- Certificate verification may be disabled only through an explicit user setting;
  the UI must describe this as insecure outside a trusted network.

Required before enabling automatic install by default:

- Sign release checksums or artifacts.
- Verify signatures against a public key embedded in the application.
- Document key rotation.

## Error handling

All recoverable failures must be typed and user-facing:

| Failure | Behavior |
|---|---|
| Offline or DNS failure | Show `Unable to check for updates. Check your network connection.` |
| GitHub rate limited | Show retry-after information when available and keep cached state. |
| Invalid release metadata | Log details, ignore that release, and continue with older compatible releases. |
| Missing platform asset | Show no compatible package for this platform. |
| Checksum mismatch | Delete the downloaded file and show a security warning. |
| Extraction failure | Delete staging directory and keep current app. |
| Install permission denied | Offer manual install/reveal downloaded artifact. |
| User cancels | Stop the operation without marking it as failed. |

No updater code may panic for network, parsing, download, verification, extraction, or
installation failures.

## Testing requirements

Unit tests:

- SemVer tag parsing and comparison.
- Stable vs preview channel filtering.
- Target triple asset selection.
- GitHub release JSON parsing with missing optional fields.
- `ETag` cache behavior.
- Checksum parsing and mismatch detection.
- Archive path traversal rejection.
- Update config load/default/quarantine behavior.

Integration tests:

- Mock GitHub release endpoint returns no update.
- Mock endpoint returns update with matching asset.
- Interrupted download leaves no installable staged update.
- Verified staged update transitions to `Ready to install`.
- Platform installer dry-run validates expected files.

Manual smoke tests:

- Windows portable directory update.
- Linux user-writable portable directory update.
- macOS `.app` bundle replacement or documented fallback.
- Manual check from Settings/About.
- Startup auto-check does not block opening the main window.

## Release workflow requirements

The release workflow must:

1. Bump `VERSION` before tagging.
2. Check out the release tag before building artifacts, so binaries embed the
   same version as the release tag.
3. Build release artifacts for every supported target triple.
4. Archive the `dist/oneterm-<version>-<triple>/` directory using the required
   asset name.
5. Create a GitHub Release with release notes and platform assets.
6. Rely on GitHub Release asset `digest` metadata for SHA-256 verification.
7. Mark prerelease versions as GitHub prereleases.
8. Keep draft releases invisible to updater clients.

## Implementation phases

1. **Release metadata phase**: standardize asset names, checksums, and GitHub Release
   conventions without adding in-app update UI.
2. **Check-only phase**: add update config, GitHub release checking, and Settings/About
   UI that reports whether an update exists.
3. **Download phase**: download matching artifacts, verify checksums, and stage
   extracted files.
4. **Install phase**: implement platform installers with rollback and restart.
5. **Signing phase**: enforce signed artifacts before enabling automatic installation
   by default.

## Acceptance criteria

- Release builds can check GitHub Releases manually and from startup without blocking
  the UI thread.
- The updater never offers an older, equal, draft, or incompatible release.
- The updater selects only the asset for the current target triple.
- Downloaded artifacts are checksum-verified before extraction and installation.
- Failed checks/downloads/installs keep the current application usable.
- User preferences persist through the shared persistence policy.
- Debug builds do not auto-check unless explicitly enabled for testing.
