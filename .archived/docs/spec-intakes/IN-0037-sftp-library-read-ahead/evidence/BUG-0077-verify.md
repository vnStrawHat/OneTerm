# BUG-0077 adversarial verification

- Subject: `909b1c50` `fix(ssh): folder transfer progress counts the whole tree (BUG-0077)`,
  one commit on top of `main` @`2fc6910e`.
- Packet: [`../BUG-0077-sftp-folder-progress-stalls-at-99.md`](../BUG-0077-sftp-folder-progress-stalls-at-99.md)
- Date: 2026-09-25. Host: Windows 11, MSVC, `CARGO_BUILD_JOBS=4`, worktree-local target dir.

## Verdict: PASS

The cause is the one the packet names, the fix removes it in both directions, the new tests fail
on `main`'s sources and pass on the fix, and the full local gate is green. The download
restructure keeps every walk check, in the same order, with the same entry count. The upload
rewrite deletes the CORR-18 channel hand-off; the CORR-18 invariant (no polling, a cancel is
seen at once) still holds, and is actually stronger now (measured below). What was knowingly
given up is the SCALE-04 bounded-memory streaming of `7789731e`, and that trade is written in
the packet. The findings below are Low/Info: none blocks the merge. A few record fixes (F6,
F7, F14, F15) are cheap to make when the packet is closed.

## Findings

| # | Severity | Finding | Evidence |
|---|---|---|---|
| F1 | PASS | **Cause confirmed.** On `main`, `sftp_download_dir` downloads each file inside the `read_dir` loop as soon as it sees it. The fraction is `bytes_done / discovered_bytes.max(bytes_done)` capped at `0.99`, and `discovered_bytes` only counts files already seen. So while the first file copies, numerator and denominator converge on 0.99, and the `candidate > reported_progress` guard holds that value until the final `Progress(1.0)`. The upload consumer does the same arithmetic on entries as it receives them. The packet's line citations for `main` match (`download.rs:196-266`, `:246-254`, `:276`). | `git show 2fc6910e:crates/ssh/src/sftp_task/transfer/download.rs`; negative control below. |
| F2 | PASS | **Download listing pass keeps every check, check by check.** Here `main` is compared with `909b1c50` `download.rs:184-238`. Root `create_dir_all` + `canonicalize` come before any remote name is trusted (unchanged, `:177-182`). Cancel is checked per directory popped (`:192-194`) and per entry (`:202-204`), both through `report_cancellation`, as before. `MAX_TRAVERSAL_DEPTH` is checked per directory (`:195-199`), as before. `.`/`..` are skipped, then `validate_remote_entry_name`, then `visited += 1` and the `MAX_TRAVERSAL_ENTRIES` check (`:205-215`): same position, so the cap counts the same things (every non-dot entry, files and directories). A symlink entry is refused (`:217-222`) **before** `safe_local_child` and before any local write for that entry, as before. `safe_local_child` (`:224`) and `create_safe_parent_dirs(.., placeholder)` for directories (`:226-230`) are unchanged. The only difference is that a file entry is pushed to `files` instead of downloaded on the spot. Pass 2 (`:240-272`) calls `create_safe_parent_dirs(&local_root, local_child)` again right before each file, and `download_file_contents` still refuses a local symlink target (`:91-95`). So a local component swapped for a link between the two passes is still caught at copy time. | Side-by-side read of both versions. |
| F3 | PASS | **A cancel during listing leaves less behind than before.** The listing pass creates only directories. A throwaway test (not committed) served 50 dirs x 1 file, cancelled once `Discovering(n >= 5)` arrived, and got `Err(Cancelled)`, the `Cancelled` event and **0 files** on disk (only empty directories). Before the fix, the same cancel point would already have left whole downloaded files. The same holds for a cap/symlink/name error found late in the tree: it now fails before any byte moves, where `main` failed after a partial download (see F13). | Scratch test `zz_download_cancel_during_listing_writes_no_file`: `download result cancelled=true saw Cancelled event=true local files=0`. |
| F4 | Info | **Cancel between files in pass 2.** Pass 2 has no explicit `is_cancelled()` check at the top of the loop. `copy_sequential` polls `cancel.cancelled()` first (`biased`, `pipeline.rs:76-80`), so the file after a cancel returns `Cancelled` on its first iteration, after one remote `open` round trip and a local `.part` create that is then removed. That cost is paid once per cancel, not per file. It is not a problem, but a one-line `if cancel.is_cancelled()` at the top of the pass-2 loop would save the round trip. | `download.rs:243-272`, `pipeline.rs:63-97`. |
| F5 | Low | **The listing-to-copy window is now the whole transfer (TOCTOU), and the packet does not mention it.** Download: a remote entry listed as a regular file can be replaced by a remote symlink before `sftp.open`, which follows it. Upload: a local file can be replaced by a symlink after the walk, and `tokio::fs::File::open` follows it. Before the fix, the window was one `readdir` batch (download) or at most 128 buffered entries (upload). Local containment is not weakened: F2's re-checks still refuse local links and escapes. The worst case is reading a file the same account could already read, into the chosen destination. This is worth one line in the packet's "Tree changes between listing and copy" gap. | `download.rs:99-103`, `upload.rs:76-78`. |
| F6 | PASS (CORR-18) / Low (records) | **CORR-18 invariant: still held.** CORR-18 (`docs/review-refresh-2026-08/02-correctness-concurrency.md:109-113`) is "Polling instead of notification": `send_local_upload_entry` spun on `thread::sleep(1ms)`. The fix was `send_blocking`, plus a test proving that a walker parked on a full channel wakes with `Cancelled` when the consumer cancels and drops the receiver (no stuck or spinning thread). It guarded against a deadlock or spin of the walker, and against a cancel that could not reach a blocked walker. With no channel, the walker cannot park any more: it is a straight loop that checks the token per directory (`upload.rs:148`) **and now per entry** (`:159`, new). Before, the check ran only per sent entry. Measured with a throwaway test on a 20 201-entry local tree: the full walk took 443 ms in a debug build, and **cancel to return took 96 µs** (`Err(Cancelled)`). A walk inside `spawn_blocking` cannot be aborted, but it does not need to be: it watches the token. The deleted test has nothing left to test. The remaining CORR-18 test (`local_traversal_stops_before_filesystem_access_when_cancelled`) was adapted and kept. **Records nit:** the packet calls CORR-18 "the upload walker's channel hand-off". The streaming design itself was SCALE-04 in `7789731e` ("Stream recursive upload discovery through a bounded producer/consumer channel and process recursive downloads incrementally without retaining a complete file plan"). The packet names the commit and "bounded memory" but not SCALE-04. Suggest naming it, since `docs/review/` is gone from the tree. | Scratch test `zz_upload_walk_cancel_is_prompt`: `full walk: 20201 entries in 443.1871ms`, `cancel->return: 96.2µs, result cancelled=true`; `git show 7789731e -- docs/review/`. |
| F7 | Low | **Bounded memory (SCALE-04) was knowingly relaxed, and the relaxation is acceptable.** Per entry: upload `LocalUploadEntry::File` is about 64 B + 2 x path; download `(String, PathBuf, FileAttributes)` is about 150 B + 2 x path. At the 100 000-entry cap with ~100-byte paths that is roughly 25-35 MB, which matches the packet's "tens of MB". The hostile worst case was already O(entries x path) on `main`: the DFS `pending` stack holds every subdirectory of a wide directory, and russh-sftp's `read_dir` materializes a whole directory's listing. The argument is in the packet (Decisions + Gaps "Memory"). `docs/sftp-browser-design.md` says only "`MAX_TRAVERSAL_ENTRIES` (100 000, which also bounds the list)". It does not say that the design moved from O(pending dirs) to O(files) or why. Suggest one sentence there, since the design doc is the contract future work reads. The design doc no longer describes streaming upload: `incremental`, `as discovered`, `producer` and `discovered so far` appear only in the historical sentence explaining the bug. | `rtk proxy grep` over `docs/sftp-browser-design.md`; `download.rs:188`, `upload.rs:125-133`. |
| F8 | Info | **Upload has no "scanning" state.** Only download sends `Discovering`. The local walk is fast (20k entries in 0.44 s debug), so the upload row sits at 0% briefly, but a slow or network local drive would freeze it at 0% for the whole walk. The packet does not say why upload has no `Discovering`. | `upload.rs:196-210`. |
| F9 | Info | **An abort without the token lets the walk finish.** If the transfer future is dropped without cancelling the token, the blocking walk runs to completion (bounded by the entry cap, so about 2 s at 100k entries). Before, the dropped receiver stopped it at the next send. Shutdown cancels tokens before aborting (`7789731e` "close waits for cooperative cancellation"), so this is theoretical. | `upload.rs:203-210`. |
| F10 | PASS | **Progress semantics.** The fraction is monotonic in both directions (`fraction > reported_progress` guard) and clamped with `min(1.0)`: a file that grew, which only upload can see because download reads `take(listed size)`, cannot exceed 1.0. A file that shrank ends early at EOF; the unconditional final `Progress(1.0)` still goes out (`download.rs:279`, `upload.rs:283`), and `Completed` comes from the result channel (`transfer.rs:363-369`, which also sets `progress = 1.0`). With `total_bytes == 0` no intermediate sample is sent, so an empty folder and a zero-byte file yield exactly `[1.0]` (test `empty_folders_and_zero_byte_files_still_complete`, both directions). Note: that test also passes on `main`. It guards against a regression but does not tell the fix apart from `main`. | Code; negative control output. |
| F11 | Low | **The bar can read 100% while the row is still in progress.** The row prints `{:.0}%`, so 0.995 shows as "100%". The true 1.0 arrives with the last chunk, before `finalize_*` (rename), metadata and remote `close` run. My upload walk caught exactly one such frame: `100%` with the cancel `x` still shown, then `Done`. The same is true of single-file transfers today (`min(1.0)`, no cap), so this is consistent and acceptable. The design doc implies it ("reaches 1.0 only with the last file's last chunk ... `Completed` comes from the result channel") but does not say it in plain words. | `render_transfer.rs:149`; `evidence/BUG-0077-verify-upload-queue-row-burst.png` frame 35. |
| F12 | PASS | **`Discovering` is handled by every consumer.** `TransferEvent` has one production consumer, `run_transfer` (`crates/sftp-ui/src/transfer.rs:342-360`), whose `match` is exhaustive with no wildcard, so a missing arm would not compile. Other users are tests that filter with `if let` / `matches!` (`in0037_verify_tests.rs`, `folder_progress_tests.rs`, sftp-ui tests) or senders (`test_backend.rs`, `crates/ssh/src/sftp.rs`). The first `Progress` clears `discovered` (`:348-351`). The scanning text is also shown only while `status == InProgress` (`render_transfer.rs:122`), so `Cancelled`/`Error`/`Completed` hide it even though `discovered` is not reset on those paths. Nit: the label reads "1 files found" for a single file. | `rtk proxy grep -rln TransferEvent crates`. |
| F13 | Info | **Behavior change not written in the packet: errors found late in the tree now fail before any byte moves.** A symlink, bad name, depth/entry cap overflow or unreadable directory deep in the tree used to fail after a partial transfer. On upload the walker's error even surfaced only after every streamed entry had been uploaded. Now it fails in the listing pass. This is an improvement (less partial state), and it is worth a line in the Outcome. | F2, F3. |
| F14 | Low | **Stale module docs.** `download.rs:1` "Incremental remote-to-local SFTP downloads." and `upload.rs:1` "Incremental local-to-remote SFTP uploads." now describe the streaming walk that was removed; the function docs were updated but not these two lines. | `rtk proxy grep -n incremental`. |
| F15 | Low | **Packet template and proof block.** The packet is dated 2026-09-25 and follows `docs/templates/work.md` except that the **Handoff** section is missing ("Reported by"/"Root cause" stand in for Context, which is fine). The owning docs reviewed are listed with actions. The gaps cover no real server, no upload GUI walk (now done here, F17), up-front listing on high-latency links, memory, tree changes, and the root-with-thousands 0% delay. All three gaps the brief asked about are present. "Platform proof" is ticked, but only the Windows local gate has run; CI's Linux/macOS jobs have not run on this commit. The evidence folder has **five** screenshots (the packet lists five; the brief said six): `before-mid-download`, `before-queue-row-burst`, `after-mid-download`, `after-queue-row-burst`, `after-scanning`. Each matches its claim: before shows 52% then 99% on 23 frames; after shows 3%..89% over 28 frames; scanning shows `many (scanning, 373 files found)` at 0% with `tree` Done above it. | `ls evidence/`; screenshots viewed. |
| F16 | PASS | **Negative control and flakiness.** See below. | |
| F17 | PASS | **Manual upload walk (not done by the implementer).** Own `sftp-dev-server` (port 2478, scratch root) and own debug `oneterm.exe` built from this worktree, with a scratch `USERPROFILE`. Local `up/one.bin` 32 MiB, `up/d1/two.bin` 32 MiB, `up/d1/d2/big.bin` 800 MiB (the big file last). SFTP Browser expanded, `up` selected in the Local pane, Upload, then 40 captures ~200 ms apart. Queue row: **1%, 4%, 6%, 8%, 11%, 14%, 17%, 20%, 23%, 26%, 29%, 32%, 34%, 37%, 40%, 42%, 45%, 47%, 50%, 53%, 56%, 59%, 62%, 65%, 68%, 72%, 75%, 78%, 81%, 85%, 88%, 91%, 93%, 96%, 100%**, then Done x5. The app log shows `sftp_upload_dir: ... → "/up" — 3 files, 905969664 bytes`. On the server: `one.bin` 33554432, `d1/two.bin` 33554432, `d1/d2/big.bin` 838860800. Only the two pids this walk started were stopped. The download walk was not repeated: the implementer's five screenshots were checked instead (F15). | [`BUG-0077-verify-upload-queue-row-burst.png`](BUG-0077-verify-upload-queue-row-burst.png) |

## Negative control

`main`'s `transfer/download.rs`, `transfer/upload.rs` and `sftp_task_tests.rs` restored with
`git checkout 2fc6910e -- ...`, keeping the new `folder_progress_tests.rs` and the
`Discovering` variant. `sftp_task_tests.rs` has to go back too, because its adapted tests call
`collect_local_upload_entries`. Command: `cargo test -p oneterm-ssh folder_progress -- --nocapture`:

```
test sftp_task::folder_progress_tests::empty_folders_and_zero_byte_files_still_complete ... ok
upload fractions: [0.249, 0.498, 0.747, 0.990, 1.000]
test sftp_task::folder_progress_tests::folder_upload_progress_counts_the_whole_tree ... FAILED
download fractions: [0.249, 0.499, 0.749, 0.990, 1.000]
test sftp_task::folder_progress_tests::folder_download_progress_counts_the_whole_tree ... FAILED
test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 107 filtered out; finished in 0.07s
```

Fixed sources (`909b1c50`):

```
upload fractions: [0.025, 0.050, 0.075, 0.100, 0.100, 0.125, 0.150, 0.175, 0.200, 0.200, 0.225, ... 0.947, 0.972, 0.997, 1.000, 1.000]
download fractions: [0.025, 0.050, 0.075, 0.100, 0.100, 0.125, 0.150, 0.175, 0.200, 0.200, 0.225, ... 0.950, 0.975, 1.000, 1.000, 1.000]
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 106 filtered out; finished in 0.08s
```

The numbers match the packet exactly.

Flakiness, `cargo test -p oneterm-ssh -p oneterm-sftp-ui` x3: every run gave
`oneterm-sftp-ui` 75 passed and `oneterm-ssh` 109 passed, 0 failed.

## Gate

`pwsh scripts/ci-local.ps1` (`CARGO_BUILD_JOBS=4`) on `909b1c50`, final line:

```
ci-local: all checks passed.
```

## Gaps

- No real-server run (loopback `sftp-dev-server` and the in-process server only).
- Grown/shrunk files and the listing-to-copy TOCTOU (F5) were argued from code, not exercised.
- The two scratch tests behind F3 and F6 were run and removed. They are not in the tree; the
  packet may want the cancel-during-listing test kept as a regression test.
- The memory figures in F7 are estimates from type sizes, not measured.
