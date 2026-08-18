# 03 — Security

> Part of the [2026-08 refresh review](README.md). Checklist format.

## Assessment

Security-relevant surfaces are centralised and mostly tested: `TerminalSecurityPolicy`,
`ExternalTargetPolicy`, `PastePolicy`, OSC 9;7 size cap + schema check, `SecretString` (zeroize +
masked `Debug`), fail-closed host-key handling with explicit fingerprint approval, SFTP local-destination
hardening (name validation, `.part` staging). The gaps are specific: one paste-policy bypass, SHA-1 RSA
signatures, a host-key edge case, symlink checks that never fire, an updater whose only trust anchor is
TLS, and remote-controlled unbounded growth in the agent dedup map.

---

## A. Terminal input / escape handling

- [x] **[High] SEC-01 — Bracketed-paste marker stripping is bypassable by nesting.**
  `crates/terminal/src/paste.rs:84-114`: single-pass removal reassembles a marker: input
  `"\x1b[20" + "\x1b[201~" + "1~"` → after stripping the inner marker the output is exactly `\x1b[201~`,
  terminating bracketed paste and letting the rest run as keystrokes.
  *Fix:* follow alacritty — strip all `\x1b` (and `\x03`) from the payload in bracketed mode — or loop until
  no marker remains; add this vector to `session.rs::bracketed_paste_strips_embedded_markers`.

- [x] **[Medium] SEC-03 — OSC 8 display/target mismatch defence is dead code.**
  `crates/terminal/src/url_policy.rs:131-144` (`validate_with_display`) has no callers;
  `crates/terminal-view/src/handlers/mouse.rs:45` uses `validate()` only, so an OSC 8 link whose visible text
  is `https://good.com` but target `https://evil.com` opens without confirmation. Also the `Confirm` policy
  outcome is a silent no-op with a `TODO` (`mouse.rs:49-56`).
  *Fix:* call `validate_with_display(target, Some(display_text))` from the click handler and show a
  confirmation (or notification) for `Confirm`; or delete the API if the product decision is otherwise.

- [x] **[Medium] SEC-04 — Per-agent dedup map is unbounded and terminal-controlled.**
  `crates/terminal/src/osc_agent/dedup.rs:311-321`: every distinct `agent` id inserts a `String` key forever;
  a hostile program can grow memory indefinitely (ids up to ~6 KiB each). *Fix:* cap tracked agents
  (LRU/evict oldest) and cap `agent` length at parse time.

- [x] **[Medium] SEC-06 — ConPTY command line built without quoting.**
  `crates/local-shell/src/session.rs:66-75` uses `Options::default()` (`escape_args: false`); alacritty joins
  program and args raw and passes `lpApplicationName = NULL` (`vendor/alacritty_terminal/src/tty/windows/mod.rs:158-174`,
  `conpty.rs:205-229`). Program paths containing spaces (`C:\Program Files\PowerShell\7\pwsh.exe`) are parsed
  ambiguously (CWE-428), and `Custom` args are unescaped. *Fix:* quote the program path; enable `escape_args`
  and express `cmd /K chcp 65001 >nul` as a single arg (or keep raw only for `Cmd`).

- [x] **[Low] SEC-07 — Control-character filters allow `\n`/`\r`/`\t` in URLs and cwd; BiDi isolates not
  stripped.** `crates/terminal/src/url_policy.rs:232`, `security_policy.rs:209`. *Fix:* reject `\n`/`\r`
  for URLs and cwd; add U+2066..2069 to the BiDi list.

- [x] **[Low] SEC-08 — Clipboard-read policy applied inconsistently.** `local-shell/src/listener.rs:344-346`
  forwards `ClipboardRead` without consulting `TerminalSecurityPolicy`; ssh does (`ssh/src/listener.rs:420-427`).
  The view's `notification_policy` is `TerminalSecurityPolicy::default()` (`terminal-view/src/view/local_view.rs:298`),
  not the user's settings. *Fix:* single sink (ARCH-01) + wire from `TerminalSettings`.

- [x] **[Low] SEC-09 — Attached-value secrets leak into completion history** (`-pSECRET`, `-uuser:pass`,
  `crates/completion/src/redact.rs:64`). *Fix:* attached-form detection for vocabulary flags.

- [x] **[Low] SEC-10 — Copy-on-select is unconditional.** `terminal-view/src/handlers/mouse.rs:111-117`
  overwrites the system clipboard on every left-button release with a selection. *Fix:* `copy_on_select`
  setting (default off on Windows).

## B. SSH / SFTP

- [x] **[High] SEC-02 — RSA private keys sign with SHA-1 (`ssh-rsa`).**
  `crates/ssh/src/session.rs:244` `PrivateKeyWithHashAlg::new(Arc::new(key), None)` — russh maps `None` to the
  legacy `ssh-rsa` (SHA-1). OpenSSH ≥ 8.8 rejects it, and the user only sees "SSH authentication failed".
  *Fix:* `let alg = handle.best_supported_rsa_hash().await?.flatten().or(Some(HashAlg::Sha512));`.

- [x] **[Medium] SEC-05 — Host key of a *different algorithm* is reported as "unknown host", not "changed".**
  `crates/ssh/src/handler.rs:229-259` relies on `check_known_hosts_path`, which returns `Ok(false)` when the
  recorded key type differs. A MITM presenting an ECDSA key for a host known by ED25519 gets a friendly
  "accept new fingerprint" prompt instead of the changed-key refusal. *Fix:* if `known_host_keys_path(host, port)`
  is non-empty but no entry matches the algorithm, return `ChangedHostKey` (or a distinct "different key type"
  error requiring stronger confirmation); optionally set `client::Config.preferred.key` to prefer known types.

- [x] **[Medium] SEC-11 — Symlink checks use `metadata()` (follows links) so `is_symlink()` is never true.**
  `crates/ssh/src/sftp_task/transfer/download.rs:47-50,68-71`, `transfer/staging.rs:61-67`,
  `transfer/upload.rs:383-384`. *Fix:* `symlink_metadata()`; make the tests run on Windows too (TEST-06).

- [x] **[Medium] SEC-12 — Recursive delete follows a symlinked root.**
  `crates/ssh/src/sftp_task/recursive_delete.rs:32-38` `read_dir(root)` without lstat: deleting a
  symlink-to-directory recursively deletes the *target's* contents; a `read_dir` failure (EACCES) is
  conflated with "not a directory". *Fix:* lstat root first; symlink/non-dir → `remove_file`; propagate errors.

- [x] **[Medium] SEC-13 — Auth failure discards `remaining_methods`; no keyboard-interactive.**
  `crates/ssh/src/session.rs:258-261`. Servers with `PasswordAuthentication no` + `KbdInteractiveAuthentication yes`
  fail with a generic message. *Fix:* fall back to `authenticate_keyboard_interactive_start` for `Password`;
  include remaining methods in the error.

- [x] **[Low] SEC-14 — No keepalive** (`client::Config::default()`, `session.rs:186`). *Fix:*
  `keepalive_interval = Some(30s)`, `keepalive_max = 3`.

- [x] **[Low] SEC-15 — Transfers do not preserve permissions/mtime** (download.rs, upload.rs). *Fix:*
  `set_metadata` after finalize.

- [x] **[Low] SEC-16 — Unbounded reads of `/etc/passwd`/`/etc/group` from an untrusted server**
  (`sftp_task/metadata.rs:44,73`). *Fix:* cap at e.g. 4 MiB.

- [ ] **[Low] SEC-17 — Host-key confirmation closure re-clones the full `SshConfig` (with secret) on every
  render** (`session-ui/src/common.rs:328-356`). *Fix:* `Rc` outside the builder closure.

## C. Updater

- [x] **[Medium] SEC-18 — Disabling certificate verification silently removes all integrity.**
  `crates/update/src/github.rs` `client()` sets `danger_accept_invalid_certs(true)`; the SHA-256 digest comes
  from the same channel, so a MITM can serve any binary with a matching digest; no signature. The settings
  description (`settings-ui/src/updates/groups.rs:75-85`) just says "Verify TLS certificates."
  *Fix:* warn loudly (log + red settings text); longer term embed a public key and verify a detached signature.

- [x] **[Low] SEC-19 — Proxy URL (may embed credentials) echoed into logs on parse failure**
  (`update/src/github.rs` `invalid update proxy URL '{proxy_url}'`). *Fix:* redact userinfo.

- [x] **[Low] SEC-20 — No size cap on download/extraction** (`github.rs download_to_file`, `archive.rs`).
  *Fix:* cap by `asset_size` and a sane extracted-bytes limit.

- [x] **[Low] SEC-21 — Zip entries: no symlink/unix-mode handling** (`archive.rs:149-177`; tar rejects links,
  zip does not). *Fix:* same guard for defence in depth.

- [x] **[Low] SEC-22 — `asset_url` not restricted to `https://`; `parse_release_version(CURRENT_VERSION)`
  silently falls back to `0.0.0`** (`manager.rs:114-115`) so a bad `VERSION` makes every release "newer".
  *Fix:* enforce https; fail loudly on unparsable current version.

- [x] **[Low] SEC-23 — Updater target repository inferred at build time from `git remote`**
  (`crates/update/build.rs:16-23`) — a fork build silently ships an updater pointing at that fork. See BUILD-12.

## D. Crash reports & persisted data

- [x] **[Low] SEC-24 — Crash reports: only `$HOME` is redacted; files use default permissions.**
  `crates/app/src/crash_report.rs:273-288,357,360-388`. Panic payloads/backtraces may include hostnames,
  remote paths, command text; on Unix reports are typically 0644. *Fix:* `mode(0o600)` on Unix (0700 on
  `crashes/`); consider redacting `user@host` patterns.

- [x] **[Low] SEC-25 — Legacy crash-report import follows symlinks** (`crash_report.rs:116-134` `fs::read` +
  `remove_file`). *Fix:* skip if `symlink_metadata().is_symlink()`.

- [ ] **[Info] No secrets are persisted by `ui_config.json`, `terminal.json`, `docks.json`. Passwords /
  passphrases in session-ui are read once into `SecretString` and inputs are cleared
  (`auth_form.rs:227-257`) — good.
