# SFTP follow Terminal CWD — Part 5: Edge cases, risks & roadmap

---

## 5.1. Edge cases

| # | Situation | Expected behavior |
|---|-----------|-------------------|
| E1 | Remote shell **doesn't** emit OSC 7 → `cwd() == None` | Button disabled + explanatory tooltip. No jump. |
| E2 | `cwd` points to a directory **without read permission** (permission denied) | `goto_path`→`stat`/`read_dir` errors → show `path_error`/error message (already exists). Keep the old cwd. |
| E3 | `cwd` is a directory that was just deleted | Same as E2 — stat error → report, don't change cwd. |
| E4 | The active tab is a **local shell** (no SFTP) | Toolbar doesn't render (`render_no_connection`) → button doesn't appear. |
| E5 | SSH has a shell but **can't open an SFTP channel** | `self.sftp == None` → button doesn't appear (or is disabled). |
| E6 | User clicks Sync while a **transfer is running** | `load_dir` only changes the listing; the transfer runs independently in the background (separate channel) → no impact. |
| E7 | `cwd` equals the directory SFTP is already in | Still `load_dir` (refresh) — acceptable; or skip if equal to save (minor optional). |
| E8 | OSC 7 returns a path with **special characters / non-UTF8** | `parse_cwd_url` already handles it in the ssh layer; `PathBuf` stays as-is; SFTP `stat` reports an error if the server rejects it. |
| E9 | Path from OSC 7 carries a **different hostname** (weird mount, container) | Only use the path part of `file://host/path`. `parse_cwd_url` already drops the host. If the host differs from the real remote, the directory may not exist → E2. |
| E10 | Rapid tab switching | `active_cwd_source` updates to the latest `set_active`; observe reads the current value → always matches the tab being viewed. |

---

## 5.2. Risks & mitigations

| Risk | Level | Mitigation |
|--------|:---:|-----------|
| **OSC 7 isn't available on many servers** making the feature "useless" for that user | Medium | Clear tooltip; docs on enabling shell integration; consider injecting shell integration on SSH login (§5.4) |
| Calling `cwd_source.cwd()` every frame in `render` (Mutex lock) | Low | Lock is extremely short (clone `Option<PathBuf>`); if needed, cache + update via observe |
| Adding the `CwdSource` trait widens the `core` API surface | Low | 1-method trait, well-documented; or use option B (weak entity) if desired |
| Auto-follow (if done) causes a flood of `read_dir` when typing many `cd`s | Medium | Debounce + only load when `path != cwd` + only when panel is active |
| Borrow checker when fetching `sftp()` + `cwd_source()` together in `set_active` | Low | Split into two `let` statements, each with its own `read(cx)` |

---

## 5.3. Testing

**Unit / logic:**
- `SshCwdSource::cwd()` reflects the `SharedState.cwd` value after setting it (simulate
  OSC 7 → update state → read back).
- `sync_to_terminal_cwd`: when `cwd_source == None` → no-op; when `Some(path)` → calls
  `goto_path(path)`.

**Manual:**
1. SSH to a server with shell integration (bash + `PROMPT_COMMAND` emitting OSC 7).
2. `cd /var/log` in the terminal → click Sync → SFTP shows `/var/log`.
3. `cd /etc` → click Sync → SFTP jumps to `/etc`.
4. SSH to a server that **doesn't** emit OSC 7 → button disabled, tooltip correct.
5. Local shell tab → no panel/button visible.
6. `cd` to a directory without read permission → click Sync → error, SFTP cwd unchanged.

**Quality gate (mandatory, per AGENTS.md §5):**
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

---

## 5.4. (Implemented) OSC 7 over SSH — silent, using `exec`

For the local shell, OneTerm generates OSC 7/133 via env at spawn (silent). For SSH, every
other approach has drawbacks:

| Approach | Silent? | Needs sshd config? | Result |
|------|:--------:|:------------------:|---------|
| Write a snippet to stdin (`channel.data`) | ❌ (PTY echoes it) | No | Tried — shows up in the terminal |
| Strip echo on the client side (`EchoSuppressor`) | ❌ (echo gets reformatted) | No | Tried — still shows up |
| `channel.set_env("PROMPT_COMMAND")` | ✅ | **Yes** (`AcceptEnv`) | Tried — server rejects it → OSC 7 lost |
| **`channel.exec(...)` then `exec` the shell** | ✅ | **No** | **Currently used** |

**The approach in use** — replace `request_shell` with `channel.exec(true, cmd)`:

```
__oneterm_osc7() { printf '\x1b]7;file://%s%s\x1b\\' "${HOSTNAME:-$(hostname)}" "$PWD"; printf '\x1b]133;A\x1b\\'; };
export -f __oneterm_osc7 2>/dev/null;
export PROMPT_COMMAND='__oneterm_osc7';
[ -f /run/motd.dynamic ] && cat /run/motd.dynamic 2>/dev/null;
[ -r /etc/motd ] && cat /etc/motd 2>/dev/null;
exec "${SHELL:-/bin/bash}" -il
```

- sshd runs this command via `$SHELL -c <cmd>` (**non-interactive** → no readline →
  **no echo**).
- Steps 1–2 define the hook + export (the function via `export -f`, and `PROMPT_COMMAND`).
- Step 3 **re-prints the MOTD**: `exec` skips the step where sshd/PAM prints the login
  banner (only runs for `request_shell`), so we `cat /run/motd.dynamic` (Ubuntu's dynamic
  MOTD cache) + `/etc/motd` ourselves (guard if the file is missing → print nothing).
- Step 4 `exec`s the interactive login shell → **inherits** the hook + `PROMPT_COMMAND` →
  emits OSC 7 + OSC 133;A before each prompt.
- **Doesn't depend on** `AcceptEnv` (unlike `set_env`).

**Remaining limitations:**
- bash-oriented (`export -f` + `PROMPT_COMMAND`). zsh/other shells: no OSC 7 but harmless.
- A `.bashrc` that overwrites `PROMPT_COMMAND` will disable the hook (most distros don't
  touch it by default).
- MOTD: restored via `/run/motd.dynamic` + `/etc/motd`. The "Last login:" line (printed
  separately by sshd) is absent. If some server's PAM still prints the MOTD for exec →
  may duplicate.
- Disable entirely: `SshConfig::shell_integration = false` → use `request_shell` as before.

---

## 5.5. Implementation roadmap

Suggested order (each step must build + clippy clean before moving on):

- [ ] **B1 — core**: add the `CwdSource` trait + `fn cwd_source()` default `None` +
  re-export. `cargo build -p oneterm-core`.
- [ ] **B2 — ssh**: `SshCwdSource` + override `cwd_source()`. Build ssh.
- [ ] **B3 — (optional) local**: override `cwd_source()` for consistency.
- [ ] **B4 — ui state**: add `AppState.active_cwd_source` + update init.
- [ ] **B5 — ui terminal**: `set_active` sets `active_cwd_source`.
- [ ] **B6 — ui sftp panel**: field `cwd_source`, observe, `sync_to_terminal_cwd`,
  `terminal_cwd`.
- [ ] **B7 — ui sftp render**: Sync button on the toolbar (disabled/tooltip by state).
- [ ] **B8 — icon**: add/pick an icon (`FolderSync` or equivalent).
- [ ] **B9 — quality gate**: fmt + clippy + build workspace; manual test per §5.3.
- [ ] **B10 — (extension)** auto-follow toggle (R7): `auto_follow` flag, wire
  `SessionEvent::Cwd`, debounce, persist.

---

## 5.6. Definition of Done — first version

- The Sync button appears on the SFTP toolbar when the active tab is an SSH tab with SFTP.
- Clicking the button → SFTP navigates to the terminal's current `cwd` (read live).
- Missing OSC 7 → button disabled + tooltip; no crash, no wrong jump.
- No layering violation (`ui` doesn't import `ssh`/`local`).
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --workspace` all pass.