# Work: SFTP folder transfer progress jumps to 99% after the first file

ID: BUG-0077
Intake: IN-0037
Created: 2026-09-25

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
- Risk lane: normal. Data loss was checked: the per-file copy (`download_file_contents`,
  `upload_file_contents`), `.part` staging and `finalize_*` are untouched; only the order of
  discovery versus copying and the progress arithmetic change. Every safety check of the walk
  (symlink refusal, remote name validation, `safe_local_child`, depth and entry caps,
  cancellation) is kept.
- Spec Intake, when required: `IN-0037` owns `transfer/download.rs` since `US-0096`. The
  streaming walk itself came from the 2026-07 review remediation (`7789731e`
  "feat(scalability): bound shared session and transfer growth", no packet). Upload has the
  same root cause and is fixed here, in one packet.

## Reported by

Owner, 2026-09-25, real server: downloading a folder, the progress bar jumps to 99% at once and
stays there until Done.

## Root cause

`sftp_download_dir` (`crates/ssh/src/sftp_task/transfer/download.rs:165-278` on `main`
@2fc6910e) walks the remote tree depth-first and downloads each file inside the `read_dir` loop
the moment it is seen (`:196-266`). The fraction is
`bytes_done / discovered_bytes.max(bytes_done)` capped at `0.99` (`:246-253`), and
`discovered_bytes` (`:238`) counts only files in directories read so far. While the first file
copies, it is the only file discovered, so numerator and denominator converge and the fraction
reaches 0.99 at that file's last chunk; the `candidate > reported_progress` guard (`:254`) keeps
it there until `Progress(1.0)` (`:276`).

`sftp_upload_dir` (`crates/ssh/src/sftp_task/transfer/upload.rs:217-342`) has the same shape: the
local walker streams entries over a bounded channel and the consumer adds a file's size to
`discovered_bytes` only when it receives that file (`:297`), with the same capped fraction
(`:303-314`). It is less visible only because the local walk is fast.

## Outcome

A folder transfer's fraction is `bytes_done / total_bytes` over the whole tree, known before
any byte moves:

- Download: a discovery pass reads the whole remote tree first (collecting
  `(remote_child, local_child, attrs)` per file, local directories created as before), then the
  download pass copies the files in discovery order. During discovery the backend sends a new
  `TransferEvent::Discovering(files_found)` once per directory read; the queue row shows
  "scanning, N files found" next to the name until the first `Progress`.
- Upload: the local walk collects its entries first (in `spawn_blocking`, same caps and
  cancellation), sums the file sizes, then creates directories and uploads files as before.
- The 0.99 cap is dropped: with a true denominator the fraction only reaches 1.0 on the last
  file's last chunk, and `Completed` is decided by the result channel, not by the fraction.
  Fractions are clamped to 1.0 (a file that grew) and stay monotonic.
- Empty folders and zero-byte files still reach Done (`total == 0` sends only the final
  `Progress(1.0)`).

## Scope

- [x] In scope: `transfer/download.rs`, `transfer/upload.rs`, `TransferEvent` in
  `crates/core/src/sftp.rs`, the queue row (`crates/sftp-ui/src/{types,transfer,render_transfer}.rs`),
  tests, `docs/sftp-browser-design.md`, the IN-0037 HLD and candidate list.
- [x] Out of scope: a progress fraction for discovery itself (its length is unknown by
  definition); per-file progress within a folder; byte-rate/ETA.

## Acceptance

- [x] Fixture tree `tree/one.bin` (1 MiB), `tree/d1/two.bin` (1 MiB), `tree/d1/d2/big.bin`
  (8 MiB), the big file discovered last whatever the server's `readdir` order. Through the real
  `sftp_download`: the fraction sequence contains ~0.1 and ~0.2 (the two small files done), then
  rises through the big file, is monotonic and ends at 1.0. Fails before the fix (0.99 after the
  first file).
- [x] Same for `sftp_upload` over the same tree.
- [x] Download sends `Discovering(3)` before its first `Progress`.
- [x] An empty folder and a folder holding a zero-byte file both download and upload to Done
  with a final `Progress(1.0)`.
- [x] The queue row shows the discovering count and clears it on the first `Progress`
  (`sftp-ui` test).
- [x] Existing `oneterm-ssh` / `oneterm-sftp-ui` tests stay green (caps, symlink, cancel,
  handle limit).
- [x] Manual GUI walk against `sftp-dev-server`: mid-transfer screenshots before and after.

## Documentation

### Owning Docs Reviewed

- `docs/sftp-browser-design.md` § 5.3.1 `TransferEvent` contract (`enum TransferEvent {
  Progress(f64), Cancelled }`) and § Pipelined transfers ("directory: monotonic
  bytes/discovered") — **update required**.
- `docs/spec-intakes/IN-0037-sftp-library-read-ahead/IN-0037.md` — candidate list gains this
  packet.
- `docs/spec-intakes/IN-0037-sftp-library-read-ahead/high-level-design.md` — describes the
  download flow step 1 ("a directory goes to `sftp_download_dir`, which calls
  `download_file_contents` per discovered file") and states the UI consumes only
  `Progress`/`Cancelled` — **update required**.
- `docs/spec-intakes/IN-0037-sftp-library-read-ahead/US-0096-library-read-ahead.md`,
  `BUG-0076-sftp-handle-limit-leak.md` — per-file copy and close contract; **no change**, this
  packet does not touch the per-file copy.
- `docs/agents/error-policy.md` — no new error path; discovery errors propagate as before.
- `docs/review-refresh-2026-08/02-correctness-concurrency.md` CORR-18 — "polling instead of
  notification": `send_local_upload_entry` spun on a 1 ms sleep; the fix was `send_blocking`,
  and its test proved a walker parked on a full channel wakes on cancel. **No change**: the
  invariant still holds. This packet deletes the channel, so the walker can no longer block at
  all; it checks the token per directory and per entry (the verifier measured 96 us from cancel
  to return on a 20 201-entry tree, `evidence/BUG-0077-verify.md` F6). The test for the blocked
  walker is deleted with the channel.
- SCALE-04 (`7789731e`, review remediation; its `docs/review/` notes are no longer in the tree)
  — "stream recursive upload discovery through a bounded producer/consumer channel and process
  recursive downloads incrementally without retaining a complete file plan". **Knowingly
  relaxed**: memory goes from O(pending directories) to O(files), bounded by
  `MAX_TRAVERSAL_ENTRIES` (100 000; an estimated 25-35 MB at the cap), in exchange for a true
  progress denominator. Recorded in `docs/sftp-browser-design.md`.

### Documentation Action

Update required: `docs/sftp-browser-design.md` (event contract, folder progress), IN-0037 HLD
and candidate list.

### Reconciliation

Docs changed: `docs/sftp-browser-design.md` (the `TransferEvent` contract lists `Discovering`
and how the row shows it; the progress bullet says "bytes of the whole tree"; a new bullet
describes the listing pass, the kept walk checks, the dropped 99% cap and the empty-folder
case), `high-level-design.md` (UI section and data-flow step 1 point here, with the row as an
ASCII sketch), `IN-0037.md` (candidate list). The no-change reasons for `US-0096`, `BUG-0076`,
`docs/agents/error-policy.md` and CORR-18 still hold. After verification the design doc also
states the memory trade (SCALE-04 relaxed) and that the bar can read 100% before Done.

## Plan

- [x] Fraction-sequence tests first over the in-process server; record the failing sequences.
- [x] Download: discovery pass, then download pass; `Discovering` event.
- [x] Upload: collect the walk, then upload with the real total.
- [x] UI row; docs; GUI walk; gates.

## Decisions

None. Materializing the file list trades the streaming walk's O(pending dirs) memory for
O(files), still bounded by the existing entry cap; recorded here, not a project-wide choice.

## Verification Plan

- Focused: `cargo test -p oneterm-ssh folder_progress`, before and after.
- Regression: `cargo test -p oneterm-ssh -p oneterm-sftp-ui`.
- Manual: `cargo run -p oneterm-tools --bin sftp-dev-server`, folder whose last-discovered file
  dominates, screenshot mid-transfer with the pre-fix and the fixed build.
- Gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/check-doc-paths.py`, `python scripts/check-english.py`,
  `pwsh scripts/ci-local.ps1` (`CARGO_BUILD_JOBS=4`).

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Regression tests

`crates/ssh/src/sftp_task/folder_progress_tests.rs`, over the in-process SFTP server of
`handle_limit_tests.rs` (its `limited_session` / `scratch` helpers are now `pub(super)`), through
the real `sftp_download` / `sftp_upload`. Fixture: `tree/one.bin` 1 MiB, `tree/d1/two.bin`
1 MiB, `tree/d1/d2/big.bin` 8 MiB; the nesting makes the big file the last one found on any
`readdir` order.

Before the fix (`main` @2fc6910e sources plus the test file and the new enum variant;
`cargo test -p oneterm-ssh folder_progress -- --nocapture`):

```
upload fractions: [0.249, 0.498, 0.747, 0.990, 1.000]
download fractions: [0.249, 0.499, 0.749, 0.990, 1.000]
test sftp_task::folder_progress_tests::empty_folders_and_zero_byte_files_still_complete ... ok
test sftp_task::folder_progress_tests::folder_upload_progress_counts_the_whole_tree ... FAILED
test sftp_task::folder_progress_tests::folder_download_progress_counts_the_whole_tree ... FAILED
test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 107 filtered out
```

Every sample belongs to `one.bin`: it climbs to 0.99 inside the first 1 MiB, and the other
9 MiB (two files) report nothing until the final 1.0.

After the fix:

```
download fractions: [0.025, 0.050, 0.075, 0.100, 0.100, 0.125, 0.150, 0.175, 0.200, 0.200,
  0.225, 0.250, ... 0.950, 0.975, 1.000, 1.000, 1.000]
upload fractions:   [0.025, 0.050, 0.075, 0.100, 0.100, 0.125, 0.150, 0.175, 0.200, 0.200,
  0.225, 0.250, ... 0.947, 0.972, 0.997, 1.000, 1.000]
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 106 filtered out
```

(44 samples each; the repeated 0.100 / 0.200 are each file's EOF report; the repeated 1.000 are
the last file's EOF report and the final event.) The download test also asserts the last event
before the first `Progress` is `Discovering(3)` and that `big.bin` arrived whole. The empty
case (an empty subfolder plus a zero-byte file, both directions) yields exactly `[1.0]` and both
land on the other side. `crates/sftp-ui/src/transfer.rs`
`download_streams_the_selected_file_to_the_chosen_path` now sends `Discovering(3)`, asserts the
row holds it, then asserts the first `Progress` clears it.

Deleted test: `local_upload_discovery_cancels_while_channel_is_full` (CORR-18): it parked the
walker on a full hand-off channel, which no longer exists. The cancel-before-access and
listing tests were kept and adapted to `collect_local_upload_entries`.

### Manual loopback walk (GUI, Windows 11, debug build)

`sftp-dev-server --port 2477 --root <scratch>/root` serving `tree/one.bin` 32 MiB,
`tree/d1/two.bin` 32 MiB, `tree/d1/d2/big.bin` 800 MiB. OneTerm run from a scratch working
directory (so `target/ssh_session.json` was a seeded scratch file) with a scratch `USERPROFILE`;
password login, right-click `tree` -> Download -> Save; the queue row captured every 250-300 ms
from the Save. Pre-fix binary built from `main`'s `crates/` sources; fixed binary from this
branch. Only the pids this walk started were stopped afterwards.

| Build | Queue row over the transfer | Evidence |
|---|---|---|
| pre-fix | 52% on the first frame (inside `one.bin`), then **99% on all 23 later frames** (~7 s, `two.bin` and `big.bin`), then Done | [`evidence/BUG-0077-before-mid-download.png`](evidence/BUG-0077-before-mid-download.png), [`evidence/BUG-0077-before-queue-row-burst.png`](evidence/BUG-0077-before-queue-row-burst.png) |
| fixed | 3%, 6%, 8%, 12%, 15%, ... 82%, 86%, 89% over 28 frames, then Done; app log `sftp_download_dir: "/tree" -> ... 3 files, 905969664 bytes` | [`evidence/BUG-0077-after-mid-download.png`](evidence/BUG-0077-after-mid-download.png), [`evidence/BUG-0077-after-queue-row-burst.png`](evidence/BUG-0077-after-queue-row-burst.png) |
| fixed, `many/` = 3000 dirs x 1 small file | `many (scanning, 75 files found)` ... `(scanning, 538 files found)` at 0% while the tree is listed, then progress, then Done; 3000 of 3000 files on disk | [`evidence/BUG-0077-after-scanning.png`](evidence/BUG-0077-after-scanning.png) |

On loopback the three-directory `tree` lists too fast for a frame to catch the scanning text,
which is why the `many/` folder was added.

### Commands

| Command | Result |
|---|---|
| `cargo test -p oneterm-ssh -p oneterm-sftp-ui` | pass: `oneterm-ssh` 109 (107 + 3 new - 1 deleted), `oneterm-sftp-ui` 75 |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `python scripts/check-doc-paths.py` | pass (207 paths, 11 documents) |
| `python scripts/check-english.py` | pass (1008 files) |
| `pwsh scripts/ci-local.ps1` (`CARGO_BUILD_JOBS=4`) | pass: final line `ci-local: all checks passed.` (first attempt, no OOM) |

### Gaps

- **No real-server run.** The owner's report came from a real server; the walk used the
  loopback dev server. The arithmetic does not depend on the server.
- **Upload was not walked in the GUI by the implementer.** The unit test covers it, and the
  verifier walked it: [`evidence/BUG-0077-verify-upload-queue-row-burst.png`](evidence/BUG-0077-verify-upload-queue-row-burst.png)
  (`evidence/BUG-0077-verify.md` F17).
- **Errors deep in the tree now fail before any byte moves.** A symlink, bad name, depth or
  entry cap overflow or unreadable directory anywhere in the tree used to fail after a partial
  transfer; the listing pass now finds it first and nothing is copied. Intended, and not a
  data-loss change.
- **The bar can read 100% before Done**: the last chunk reports 1.0 before the final rename,
  metadata and close run; Done comes from the result channel (single files already did this).
- **Listing a large remote tree now happens up front**: total time is unchanged (the same
  `readdir` round trips, still sequential), but on a high-latency link a tree of many
  directories shows "scanning" for the whole listing before the first byte moves, where the
  old walk interleaved. The row says so instead of freezing at 0%.
- **Tree changes between listing and copy**: a file that grew is clamped at 1.0; a file deleted
  after listing fails that transfer (as a vanished file did before); new files are not picked up.
  The window between listing and copying is now the whole transfer (it was one `readdir` batch,
  or at most 128 buffered upload entries): an entry swapped for a symlink in that window is
  followed by the remote `open` (download) or local `File::open` (upload). Local containment is
  intact (the local link and escape checks still run at copy time); the worst case is reading a
  file the same account could already read into the chosen destination (verify F5).
- **Memory**: see SCALE-04 above; `docs/sftp-browser-design.md` states the trade.
- The first `Discovering` is sent only after the root directory is read, so a root with
  thousands of entries shows 0% without the scanning text for that first read (~1 s for 3000
  entries on loopback, most of it creating the local directories).
- **Platform proof is open**: only the Windows `ci-local` gate ran; CI's Linux and macOS jobs
  supply the rest.

## Handoff

- State: implemented on `fix/sftp-folder-progress`; independent verification PASS with no
  blocker (`evidence/BUG-0077-verify.md`, F1-F17). Its Low/nit items (F5-F7, F11-F15) are
  addressed in this packet, the design doc, the module docs and the queue-row label.
- Next owner/action: coordinator merges to `main`; CI supplies the Linux/macOS platform proof;
  the owner re-tries a folder download on the real server that showed the 99% stall.
- Blockers: none.
