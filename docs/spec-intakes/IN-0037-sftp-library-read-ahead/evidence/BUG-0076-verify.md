# BUG-0076 adversarial verification

- Subject: `85c142c6` `fix(ssh): close every remote SFTP file so the handle count cannot leak`
  on top of `main` @`0da775bf`.
- Packet: [`../BUG-0076-sftp-handle-limit-leak.md`](../BUG-0076-sftp-handle-limit-leak.md)
- Date: 2026-09-24. Host: Windows 11, MSVC, `CARGO_BUILD_JOBS=4`, worktree-local target dir.

## Verdict: PASS

The cause is what the implementer says it is, every production site that opens a remote handle
now closes it with an awaited `File::close()`, the regression tests fail before the fix and pass
after, and the full local gate is green. The findings below are Low/Info: none blocks the
merge. One packet line (the "ci-local blocked" gap) is stale after this run and should be
updated when the packet is closed.

## Findings

| # | Severity | Finding | Evidence |
|---|---|---|---|
| F1 | PASS | **Cause confirmed in russh-sftp 3.0.0.** `RawSftpSession::open` refuses with `"handle limit reached"` once `handles >= limits.open_handles` (`rawsession.rs:296`, `:307`) and increments on a `Handle` reply (`:325`); `opendir` does the same (`:543`, `:549` capital "Handle", `:565`). The only decrement is in `close` on `StatusCode::Ok` (`:331-362`; the packet cites `:331-360`, which stops at the end of the decrement block, a nit). `close_nowait` (`:364-368`) only sends. `impl Drop for File` (`fs/file.rs:272-280`) calls `close_nowait` unless `closed`. `File::close` (`:222-224`) is `shutdown`, whose `poll_shutdown` (`:450-480`) drains write acks (`:458`), awaits `session.close` and sets `closed` only on `Ok`. The limit comes from the server's `limits@openssh.com` reply (`session.rs:71-78`). The library's own `read_dir` closes with an awaited close on success (`session.rs:202`) and returns before it on a `readdir` error (`:198`); its `read`/`write` helpers close too. | Read the three files; line numbers printed directly. |
| F2 | PASS | **No fourth site.** Every `.open(`, `.create(`, `open_with_flags`, `opendir`, `read_dir(`, `sftp.read(`/`sftp.write(` under `crates/` was listed. Production handle opens are exactly `metadata.rs:46` (`read_bounded`), `transfer/download.rs:100`, `transfer/upload.rs:85`, all fixed. Directory listings (`download.rs:199`, `recursive_delete.rs:41`, `metadata.rs:185`) use the library `read_dir`. `transfer/staging.rs` only does `symlink_metadata`/`rename`/`remove_file`; `transfer.rs` only `set_metadata`; `crates/ssh/src/sftp.rs` is the command front (`read_dir` there is a test at `:249`); `crates/sftp-ui` has no russh-sftp dependency and reaches the backend through commands: the remote-edit path (IN-0011) calls `sftp.download` (`edit.rs:370`) and `sftp.upload` (`edit.rs:744`), i.e. the fixed functions, and `transfer.rs:429`/`panel_ops.rs:64` are `ReadDir` commands. Remaining dropped `File`s are in test modules (`in0037_verify_tests`, `pipeline_budget_tests`, `us0095_verify_tests`) over servers that advertise no limit. | `grep -rn -E "\.open\(|\.create\(|open_with_flags|opendir|\.read_dir\(|sftp\.read\(|sftp\.write\(" crates/` and a `russh_sftp` user list. |
| F3 | PASS | **Download success path: the close comes after the data is durable.** The close is issued after the `transfer_result` block, which ends with `flush` and `sync_all` of the `.part` file; the local file is dropped at the end of that block. A failed close is `report_best_effort` (warn, operation named) and `finalize_local_file` still runs, which is what `docs/agents/error-policy.md` asks for an optional cleanup that cannot change the result. Side improvement: `temporary_local_sibling` now runs before the open and the local `create` moved inside the block, so a failed local create also closes the remote handle. | `download.rs:97-149`. |
| F4 | Low | **The failure/cancel close is detached and can log noise at teardown.** `tokio::spawn` runs on the runtime of the calling task (the `sftp_task` `JoinSet` task on the SSH runtime); `File` is `Send`, so no panic path. It is not tracked by `sftp_task`'s `background_tasks` or its 5 s drain. If the session ends first, `close` fails fast (`send_bytes` returns "session closed", or `SessionInner::drop` clears `requests` so the pending request resolves "sender dropped"), otherwise the 10 s request timeout bounds it. Cost: one `warn` "sftp download: close remote file after failed copy: best-effort operation failed" per transfer cancelled by a disconnect. The task holds an `Arc<RawSftpSession>` only until then. Acceptable; worth knowing when reading logs. | `download.rs:134-140`; `rawsession.rs:243-246`, `:104-109` (`Drop for SessionInner`), `:255` (timeout). |
| F5 | Low | **The cancel test has a second timing dependency besides the poll.** Its first loop asserts five `Err(Cancelled)` in a row against a limit of 2, which only holds if each spawned close lands before the next-but-one `open`. On the `current_thread` test runtime it does (the spawned task sends CLOSE while the caller awaits the `.part` removal and the next `stat` round trip), deterministically in practice: **200 of 200** repeated runs of `handle_limit` passed. The final poll (10 ms steps, 5 s deadline) only accepts "handle limit reached" while waiting and fails on any other error, so it cannot pass vacuously. Not a flake today; a change to a multi-thread runtime would need a look. | Stress loop below. |
| F6 | Info | **A slow close on success leaks a unit instead of failing.** At EOF the CLOSE reply queues behind the read-ahead already issued: past-EOF replies (tiny) normally, but up to the full read budget (~4.2 MB) if the file grew after `stat` (`take(announced)` stops reading, the library keeps its reads in flight). If that exceeds the 10 s request timeout, `close` returns `Timeout`, the download still succeeds (correct), and `Drop` sends `close_nowait`, so the count keeps that one unit. The same timeout bound already applies to every READ in the pipeline, so this is not a new failure mode, and the packet's "close latency on success" gap covers the common case; the grown-file case is not written down. | `download.rs:129-133`; `rawsession.rs:255-262`. |
| F7 | PASS | **Upload ordering kept.** `remote_file.close().await` runs on both paths **before** `sftp.remove_file(&temporary)`, so REMOVE still follows CLOSE on the wire. A failed close after a good copy fails the upload with `close remote: ...` and the `.part` is removed; after a failed copy the close error is logged and the copy error returned. Cancel latency is unchanged in practice: `close` first drains the pending write acks (<= 8 x 256 KiB), but before the fix the REMOVE reply already queued behind the same writes. The residual leak (a failed write ack returned by `poll_shutdown` at `fs/file.rs:458` before CLOSE is sent) is recorded as a gap and in the upstream report. | `upload.rs:85-119`. |
| F8 | PASS | **No double close on the happy path.** `File::close(self)` consumes the file; `poll_shutdown` sets `closed = true` on `Ok`, so `Drop` returns early. Only when `close` fails does `Drop` send a second CLOSE (`closed` stays false), whose reply is ignored (`Request` is dropped at once, `SessionInner::reply` logs it at debug). Harmless. The metadata path closes after `read_to_end` through `(&mut file).take(..)`, and still closes when the read fails (`read?` is evaluated after the close). | `fs/file.rs:222-224`, `:272-280`, `:450-480`; `metadata.rs:44-57`. |
| F9 | PASS | **Tests are real and the harness enforces the limit.** `the_harness_reaches_russh_sftps_handle_limit` proves a third concurrent `open` fails with "handle limit reached" and that awaited closes give the units back, over a server whose `init` advertises `limits@openssh.com` and whose `extended` returns `max_open_handles: 2`, via OneTerm's own `sftp_config()`. The other four tests drive the real `sftp_download`, `sftp_upload` and `load_uid_gid_lookup`. Negative control reproduced independently (below). | `handle_limit_tests.rs`. |
| F10 | PASS | **Records.** Packet created 2026-09-24 from the work template (Status, Classification, Outcome, Scope, Acceptance, Documentation with Owning Docs Reviewed / Action / Reconciliation, Plan, Decisions, Verification Plan, proof block, Evidence and Gaps, Handoff; the optional Context section is replaced by "Root cause"). Owning docs reviewed include `US-0096` (IN-0037) and `US-0095` + `low-level-design/upgrade.md` (IN-0036) with a no-change reason. Gaps list the upstream `poll_shutdown` leak and the `read_dir` early-return leak, plus the async cancel window and close latency. | Packet. |
| F11 | Low | **One packet gap is stale after this run.** Its first gap and the Commands row say the full `ci-local` could not run (host OOM). It ran green here on `85c142c6` (see Gate); update the row and tick "Verify command passed" when closing. | This file, Gate. |
| F12 | PASS | **Docs.** `docs/sftp-browser-design.md` no longer says the handle is closed without awaiting; the new bullet states the close rule per direction and matches the code. The IN-0037 HLD marks its `Drop for File` bullet and risk row superseded by `BUG-0076`; `IN-0037.md` lists the packet. The `US-0096` evidence files still quote the old behaviour, which is correct for a dated record. Nuance, not an error: after a cancel the reader lives on in the spawned close until the CLOSE reply, so its pending reads de-register then, not at the cancel. | `git diff 0da775bf 85c142c6 -- docs/`; grep of `docs/` for "without awaiting" / `close_nowait`. |
| F13 | PASS (partial re-run) | **Manual.** Both screenshots exist and match the claim: before, the queue row for `bulk` shows **Error** "Limit exceeded: handle limit reached"; after, **Done**; same session `bug0076-dev` `dev@127.0.0.1:2244`. The GUI walk was not repeated here. `sftp-dev-server --port 2391 --max-open-handles 16` builds and serves; `--max-open-handles abc` prints the usage line and exits 2. | `evidence/BUG-0076-{before,after}-folder-download.png`. |

## Negative control

The three sources restored from `0da775bf`, `handle_limit_tests.rs` kept,
`cargo test -p oneterm-ssh handle_limit`:

```
test sftp_task::handle_limit_tests::the_harness_reaches_russh_sftps_handle_limit ... ok
test sftp_task::handle_limit_tests::a_cancelled_download_gives_its_handle_back ... FAILED
test sftp_task::handle_limit_tests::the_uid_gid_lookup_gives_its_handles_back ... FAILED
test sftp_task::handle_limit_tests::a_folder_with_more_files_than_the_handle_limit_uploads ... FAILED
test sftp_task::handle_limit_tests::a_folder_with_more_files_than_the_handle_limit_downloads ... FAILED
expected a cancellation, got Err(Other("Limit exceeded: handle limit reached"))
download after the uid/gid lookup: Other("Limit exceeded: handle limit reached")
folder upload: Other("Limit exceeded: handle limit reached")
folder download: Other("Limit exceeded: handle limit reached")
test result: FAILED. 1 passed; 4 failed; 0 ignored; 0 measured; 102 filtered out; finished in 0.02s
```

Sources restored to `85c142c6` afterwards (`git status` clean).

## Fixed runs

- `cargo test -p oneterm-ssh`, three runs: `107 passed; 0 failed` each (0.92 s, 0.89 s, 0.98 s).
- The `oneterm_ssh` lib test binary with filter `handle_limit --test-threads=8`, 200 runs:
  **0 failures**.

## Gate

`pwsh scripts/ci-local.ps1` with `CARGO_BUILD_JOBS=4` (no OOM at 4 jobs), on `85c142c6`:

```
ci-local: all checks passed.
```

## Gaps

- No run against a real OpenSSH `sftp-server`; the limit is exercised through the same
  `limits@openssh.com` exchange but only against OneTerm's test and dev servers.
- The GUI walk (300 files, limit 16) was not repeated; the screenshots were checked.
- F4 and F6 are unrecorded in the packet; they are logging and edge-case notes, not defects.
- The upstream leaks (`poll_shutdown` failed write ack, `read_dir` error path) remain until
  russh-sftp fixes them; OneTerm cannot close them without `[patch]`.
