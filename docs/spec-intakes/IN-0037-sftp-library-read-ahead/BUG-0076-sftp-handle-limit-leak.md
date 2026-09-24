# Work: SFTP transfers leak russh-sftp's client-side handle count

ID: BUG-0076
Intake: IN-0037
Created: 2026-09-24

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug (shipped behavior)
- Risk lane: normal. Data loss was checked: the fix only adds an awaited `CLOSE` after the
  copy; the `.part` staging and `finalize_*` contracts are untouched, and no path that used to
  fail now succeeds with fewer bytes.
- Spec Intake, when required: `IN-0037` (owns the download path via `US-0096`). The upload
  site (`US-0095` / `IN-0036` pinned its write budget) and the uid/gid lookup site are the same
  root cause and are fixed here, in one packet.

## Reported by

Owner, 2026-09-24: downloading a folder over SFTP fails with
`Limit exceeded: handle limit reached`.

## Root cause

russh-sftp 3.0.0 keeps its own count of open handles and refuses to open past the server's
`limits@openssh.com` `max_open_handles`
(`russh-sftp-3.0.0/src/client/rawsession.rs:296-329` `open`, `:543-566` `opendir`; the string is
built at `:307` and `:549`). The count is incremented on every `Handle` reply (`:325`, `:565`) and
decremented **only** in `RawSftpSession::close(...).await` on a `Status::Ok` reply (`:331-360`).
`impl Drop for File` (`client/fs/file.rs:272-280`) sends `close_nowait` (`rawsession.rs:364-368`),
which the server honours but which never touches the counter. `File::close`
(`fs/file.rs:222-224`) -> `poll_shutdown` (`:450-480`) is the only path that awaits
`session.close` and decrements.

OneTerm never awaited a close:

- `crates/ssh/src/sftp_task/transfer/download.rs:98-102` opens, wraps in `take(announced)` and
  drops the reader.
- `crates/ssh/src/sftp_task/transfer/upload.rs:85-96` flushes and `drop(remote_file)`.
- `crates/ssh/src/sftp_task/metadata.rs:44-52` `read_bounded` drops the file after
  `read_to_end` (two leaks at every SFTP session start: `/etc/passwd`, `/etc/group`).

So every transferred file leaks one unit; once `max_open_handles` files have been transferred in
one SFTP session, every later `open`/`opendir` in that session fails, including single-file
transfers and directory listings. `read_dir` in the library closes its own handle on success
(`client/session.rs:180-204`).

## Outcome

Every remote `File` OneTerm opens is closed with an awaited `File::close()` on the success,
failure and cancel paths, so a session's handle count returns to zero after each transfer and a
folder with more files than `max_open_handles` downloads and uploads completely.

Close errors, per `docs/agents/error-policy.md`:

- **Download:** once the bytes are fully read, flushed and `sync_all`ed into the `.part`
  sibling, a failed close of the *read* handle cannot change the data; it is logged
  (`report_best_effort`, warn, operation named) and the download succeeds. On a failed or
  cancelled copy the close runs in a spawned task (best effort, logged) and the original error
  is returned at once: the CLOSE reply queues behind up to the whole read-ahead budget
  (~4.2 MB), and a cancel must not wait for that to cross a slow link.
- **Upload:** close is part of the write contract (it drains pending write acks and a server
  may report a deferred write error at close), so a failed close **fails the upload** and the
  remote `.part` temporary is removed, exactly like a failed write. The close is awaited on the
  failure path too, so the temporary's REMOVE still reaches the server after its CLOSE (as the
  dropped handle's `close_nowait` used to guarantee).
- **uid/gid lookup:** already best effort; a failed close is logged and the data kept.

## Scope

- [x] In scope: `transfer/download.rs`, `transfer/upload.rs`, `metadata.rs` (`read_bounded`);
  a regression test over an in-process SFTP server advertising `max_open_handles = 2`;
  `docs/sftp-browser-design.md` (the "teardown is just dropping the reader" line); an opt-in
  `--max-open-handles` flag on the `sftp-dev-server` tool for the manual walk.
- [x] Out of scope: patching russh-sftp (`docs/PROJECT.md` forbids `[patch]`/vendored source);
  the library's own `read_dir` leak on a `readdir` error; `max_concurrent_*` budgets.

## Acceptance

- [x] Through the real `sftp_download`, a remote directory of 5 files downloads completely over a
  session whose server advertises `max_open_handles = 2`, and a later single-file download in the
  same session still succeeds.
- [x] Through the real `sftp_upload`, 5 files upload sequentially over the same kind of session.
- [x] After `load_uid_gid_lookup` (2 opens), a download in the same session still succeeds.
- [x] Each of the three tests fails with `handle limit reached` before the fix.
- [x] A cancelled download also releases its handle (the next download in the session works).
- [x] Existing `oneterm-ssh` tests (cancel, dead transport, failing sink, budgets) stay green.

## Documentation

### Owning Docs Reviewed

- `docs/sftp-browser-design.md` § Pipelined transfers — **changed**: it said cancel teardown "is
  just dropping the reader ... closes the handle without awaiting", which is the defect.
- `docs/spec-intakes/IN-0037-sftp-library-read-ahead/IN-0037.md`, `high-level-design.md`,
  `US-0096-library-read-ahead.md` — owning intake/packet of the download path. HLD reviewed:
  it describes the read-ahead and cancel teardown in terms of the library `Drop`; **changed**
  with one line pointing at the awaited close. `IN-0037.md` candidate list gains this packet.
- `docs/spec-intakes/IN-0036-russh-0-63/US-0095-russh-0-63-bump.md` and
  `low-level-design/upgrade.md` — owner of the upload write budget. **No change**: the budget is
  unchanged; the added close only drains the same writes `flush` already drained.
- `docs/agents/error-policy.md` — close-error handling above follows its decision table
  (optional cleanup logged with the operation name; a write-contract failure is an error).
- `docs/PROJECT.md` — no `[patch]`, no vendored third-party source: the fix stays in OneTerm and
  the library defect goes to Handoff as an upstream report.
- `docs/ssh-client-connect.md` — session setup; **no change**, it does not describe handles.

### Documentation Action

Update required: `docs/sftp-browser-design.md` (transfer teardown), the IN-0037 HLD cancel note,
the IN-0037 candidate list.

### Reconciliation

Docs changed: `docs/sftp-browser-design.md` (the teardown bullet no longer says the handle is
closed "without awaiting"; a new bullet states the close rule per direction),
`high-level-design.md` (the `Drop for File` bullet and the cancellation risk row point here),
`IN-0037.md` (candidate list). The no-change reasons for the IN-0036 records and
`docs/ssh-client-connect.md` still hold: no budget and no session setup changed.

## Plan

- [x] Regression test first; record its failing output.
- [x] Await `File::close()` at the three sites.
- [x] Docs, dev-server flag, manual loopback walk, gates.

## Decisions

None. Close-error handling is local policy application, recorded above.

## Verification Plan

- Focused: `cargo test -p oneterm-ssh handle_limit` before and after the fix.
- Regression: `cargo test -p oneterm-ssh`.
- Manual: `sftp-dev-server --max-open-handles 16`, download a folder of 300 small files, with
  the pre-fix and the fixed build.
- Gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/check-doc-paths.py`, `python scripts/check-english.py`,
  `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Regression test

`crates/ssh/src/sftp_task/handle_limit_tests.rs`: an in-process `SftpSession` built with
OneTerm's `sftp_config()` over a `tokio::io::duplex` to a file-system SFTP server rooted in a
scratch directory. `Limits` / `set_limits` are **not** reachable from a high-level
`SftpSession` (`Features` is `pub(crate)`), so the honest path is the protocol one: the server's
`init` advertises `limits@openssh.com` "1" and its `extended` handler answers with a
`LimitsExtension { max_open_handles: 2, .. }`, which `SftpSession::new_with_config` reads and
applies (`client/session.rs:71-78`) exactly as it does against OpenSSH. A fifth test,
`the_harness_reaches_russh_sftps_handle_limit`, proves the limit is live (a third concurrent
`open` fails, `close().await` gives the units back), so the other four cannot pass vacuously.
All four drive OneTerm's real functions: `sftp_download` (folder of 5, then a single file),
`sftp_upload` (folder of 5), `load_uid_gid_lookup` then `sftp_download`, and five cancelled
`sftp_download`s then a real one.

Before the fix (`main` @0da775bf plus the test file only; the cancel test's last step was later
changed to poll, see below, but it fails inside the cancel loop, which is unchanged):

```
running 5 tests
test sftp_task::handle_limit_tests::the_harness_reaches_russh_sftps_handle_limit ... ok
test sftp_task::handle_limit_tests::a_cancelled_download_gives_its_handle_back ... FAILED
test sftp_task::handle_limit_tests::a_folder_with_more_files_than_the_handle_limit_uploads ... FAILED
test sftp_task::handle_limit_tests::the_uid_gid_lookup_gives_its_handles_back ... FAILED
test sftp_task::handle_limit_tests::a_folder_with_more_files_than_the_handle_limit_downloads ... FAILED
---- a_cancelled_download_gives_its_handle_back ----
expected a cancellation, got Err(Other("Limit exceeded: handle limit reached"))
---- a_folder_with_more_files_than_the_handle_limit_uploads ----
folder upload: Other("Limit exceeded: handle limit reached")
---- the_uid_gid_lookup_gives_its_handles_back ----
download after the uid/gid lookup: Other("Limit exceeded: handle limit reached")
---- a_folder_with_more_files_than_the_handle_limit_downloads ----
folder download: Other("Limit exceeded: handle limit reached")
test result: FAILED. 1 passed; 4 failed; 0 ignored; 0 measured; 102 filtered out; finished in 0.04s
```

After the fix:

```
running 5 tests
test sftp_task::handle_limit_tests::the_harness_reaches_russh_sftps_handle_limit ... ok
test sftp_task::handle_limit_tests::the_uid_gid_lookup_gives_its_handles_back ... ok
test sftp_task::handle_limit_tests::a_cancelled_download_gives_its_handle_back ... ok
test sftp_task::handle_limit_tests::a_folder_with_more_files_than_the_handle_limit_uploads ... ok
test sftp_task::handle_limit_tests::a_folder_with_more_files_than_the_handle_limit_downloads ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 102 filtered out; finished in 0.05s
```

The cancel test polls (10 ms, 5 s deadline) for the count to come back, because the fix closes a
cancelled download's handle in a spawned task; any error other than "handle limit reached"
fails it at once, and before the fix it never comes back.

### Manual loopback walk (GUI, Windows 11, `fast-dev`)

`sftp-dev-server` gained an opt-in `--max-open-handles N` flag that advertises
`limits@openssh.com` like OpenSSH (without it the server advertises nothing and the client
enforces no limit, which is why no earlier walk could have seen this). Served a scratch root
with `bulk/` holding 300 files of ~15 bytes, `--max-open-handles 16`; OneTerm launched with a
scratch `USERPROFILE`, connected with password auth, right-click `bulk` -> Download -> Save.

| Build | Result | Server log |
|---|---|---|
| pre-fix (the three sources restored from `main`, rebuilt) | Queue row **Error: "Limit exceeded: handle limit reached"**; 16 of 300 files on disk; app log `SftpPanel: transfer #0 failed: Limit exceeded: handle limit reached` - the owner's report, reproduced | `sftp limits@openssh.com -> max_open_handles 16`; 18 `open` lines, then nothing |
| fixed | Queue row **Done**; 300 of 300 files on disk; app log `sftp_download_dir: "/bulk" -> ... 300 files, 4392 bytes` | 302 `open`, 302 `close` |

Screenshots: [`evidence/BUG-0076-before-folder-download.png`](evidence/BUG-0076-before-folder-download.png),
[`evidence/BUG-0076-after-folder-download.png`](evidence/BUG-0076-after-folder-download.png).
Afterwards the app and server (own pids only) were stopped and the seeded
`target/ssh_session.json` was deleted.

### Commands

| Command | Result |
|---|---|
| `cargo test -p oneterm-ssh` | 107 passed (102 before + 5 new) |
| `cargo fmt --all -- --check` | clean |
| `python scripts/check-doc-paths.py` | pass (207 paths, 11 documents) |
| `python scripts/check-english.py` | pass (1005 files) |
| `pwsh scripts/ci-local.ps1` (`CARGO_BUILD_JOBS=6`) | **Blocked by the host, not by a check.** Steps 1-3 passed (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, the `terminal-diagnostics` clippy). Step 4 `cargo test --workspace` never ran a test: rustc/link crashed compiling third-party crates (`hyper_util` STATUS_STACK_BUFFER_OVERRUN, `LNK1102: out of memory`, `gpui-pre` "memory allocation of 2088960 bytes failed"). Retried at 4, 2 and 1 jobs; the machine's commit charge had ~1.2-1.5 GB free (other processes, none of this session's). Final line of every run: `ci-local: FAILED: cargo test --workspace` |
| `cargo test -p oneterm-tools` | pass (the changed `sftp-dev-server` builds; 14 + 2 tests) |
| `python scripts/verify-dependency-graph.py`, `python scripts/third-party-notices.py --check` | pass |
| `python scripts/check-ignored-tests.py` | not run to completion: its `cargo test --list` needs the same gpui build (no `#[ignore]` was added) |

### Gaps

- ~~The full `ci-local` gate is not green here~~ **Closed 2026-09-24 by the verifier**: the
  gate was rerun on the same host once memory was free (`CARGO_BUILD_JOBS=4`) and ended with
  `ci-local: all checks passed.` on `85c142c6` (see `evidence/BUG-0076-verify.md`). The
  earlier failures were compiler/linker out-of-memory crashes in third-party crates, not checks.
- **Detached close after a cancelled or failed download** (verify F4): the close runs in a
  spawned task on the transfer runtime; at disconnect it fails fast ("session closed" /
  "sender dropped") or is capped by the 10 s request timeout. Cost: one warning log per transfer
  cancelled by a disconnect; the transfer shutdown loop does not wait for it.
- **A file that grows after it was measured** (verify F6): the success-path close can wait
  behind up to about 4.2 MB of read-ahead; if that exceeds the 10 s timeout the download still
  succeeds but one handle stays counted for the session. Not observed; recorded, not fixed.
- **No real OpenSSH run.** The limit is exercised through the same `limits@openssh.com`
  exchange OpenSSH uses, but against OneTerm's test/dev servers, not `sftp-server`.
- **A failed upload write can still leak one unit.** `File::close` -> `poll_shutdown` first
  drains pending write acks and returns the first failed one *before* sending CLOSE
  (`fs/file.rs:458`); the `File` is then dropped and `close_nowait` does not decrement. Only on
  a write that the server already rejected; each such failed upload leaks one unit. Library
  behaviour; part of the upstream report below.
- **The library's own `read_dir` leaks on a `readdir` error** (`client/session.rs:198` returns
  before the `close` at `:202`). Not OneTerm code; in the upstream report.
- **A cancelled download's handle comes back asynchronously.** Between the cancel and the
  spawned close's reply the count is one higher; a transfer started in that window at exactly
  `limit - 1` open handles could still see the limit. Sequential transfers never get near it.
- **Close latency on success.** A download now waits for one CLOSE round trip per file (plus the
  past-EOF read replies already queued ahead of it). Small on any link; not measured on a real RTT.

## Handoff

Upstream report for russh-sftp 3.0.0 (the owner may file it; no `[patch]` here per
`docs/PROJECT.md`):

- `src/client/fs/file.rs:272-280` `impl Drop for File` calls
  `self.session.close_nowait(...)` (`src/client/rawsession.rs:364-368`), which sends
  `SSH_FXP_CLOSE` but never decrements `RawSftpSession::handles`.
- The count is incremented in `open` (`rawsession.rs:325`) and `opendir` (`:565`) and
  decremented only in `close` on a `StatusCode::Ok` reply (`:331-360`).
- `open` (`:302-308`) and `opendir` (`:544-550`) refuse with
  `Error::Limited("handle limit reached")` once `handles >= limits.open_handles`, which comes from
  the server's `limits@openssh.com` reply (`client/session.rs:71-78`).
- Result: every `File` dropped without `.close().await` permanently consumes one unit; after
  `max_open_handles` such files every `open`/`opendir` on the session fails. Suggested fix:
  decrement when the `close_nowait` reply arrives (or on send), and close the handle on the
  error path of `SftpSession::read_dir` (`client/session.rs:198`). Also `poll_shutdown`
  (`fs/file.rs:458`) returns a failed write ack before sending CLOSE, so a failed write leaks the
  unit even when the caller does `close().await`.

Once upstream decrements on drop, the awaited closes here stay correct (they are what the
library documents) and need no revert.
