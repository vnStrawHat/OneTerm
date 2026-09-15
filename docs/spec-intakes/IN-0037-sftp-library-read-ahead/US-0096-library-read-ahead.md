# Work: SFTP downloads read sequentially on russh-sftp's pipelined read-ahead

ID: US-0096
Intake: IN-0037
Created: 2026-09-15

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

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0037`

## Outcome

An SFTP download reads its remote file straight through **one** handle and lets russh-sftp keep
`max_concurrent_reads` `SSH_FXP_READ` packets on the wire. `copy_striped`, `read_chunk`,
`read_handles_for`, `REORDER_WINDOW` and `READ_PIPELINE_DEPTH` no longer exist.
`session::sftp_config()` sets `max_concurrent_reads: 16`.

Cancellation, progress cadence, the announced-length cap and the shrinking-file/EOF clamp all
behave exactly as they did, each under a test that fails if they do not.

## Scope

- [x] In scope: `transfer::pipeline` (delete the striped path, threshold the progress cadence),
  `transfer::download` (one handle, `take(total)`), `session::sftp_config`
  (`max_concurrent_reads`), `pipeline_budget_tests` (assert the new contract, keep the D2
  regression the other way round), `docs/sftp-browser-design.md`, the IN-0036 LLD note.
- [x] Out of scope: **uploads** — `US-0095` Change F pinned `max_write_packet_len: 262_144` and
  `max_concurrent_writes: 8` on measured evidence and neither is touched here; `CHUNK_LEN`;
  `crates/sftp-ui` (no code change); resume-from-offset downloads (not implemented today, see
  Gaps); `finalize_local_file` and the `.part` staging contract.

## Acceptance

- [x] A 5 MiB loopback download costs one server READ per packet-length chunk and serves 1.00x
  the file size — no amplification, no wasted request.
- [x] The in-flight read budget is at least `16 x` one read packet (~4.2 MB), up from striping's
  ~1.04 MB.
- [x] `max_concurrent_reads: 1` is now the *regression* case and the budget test says so.
- [x] A cancel is observed within one `CHUNK_LEN` chunk; no byte written after it; the session
  stays usable for a further transfer (no leaked or blocking teardown).
- [x] A download of a file that shrank mid-transfer ends at EOF without error and reports the
  real final size.
- [x] A zero-length file and a file exactly one packet long both download byte-exactly.
- [x] A file longer than its announced size is truncated to the announced size (no over-read).
- [x] A 5 MiB download emits exactly 21 progress samples from the pipeline, and uploads emit the
  byte-identical sequence they emitted before.
- [x] Uploads are unchanged: the Change F write-budget test passes untouched.
- [x] `copy_striped`, `read_chunk`, `read_handles_for`, `REORDER_WINDOW`, `READ_PIPELINE_DEPTH`
  are absent from the tree.

### Measured

Same file, same process, back to back. Full tables and method in
[`evidence/US-0096-measurements.md`](evidence/US-0096-measurements.md).

| | READs | bytes served | ratio | in flight | wall |
|---|---:|---:|---:|---:|---:|
| **5 MiB** striping (before) | 21 | 5 263 100 | 1.00x | 1 044 480 | 10.26 ms |
| **5 MiB** read-ahead 16 (after) | 21 | 5 242 880 | 1.00x | **4 194 096** | **9.66 ms** |
| **50 MiB** striping (before) | 201 | 52 631 000 | 1.00x | 1 044 480 | 107.56 ms |
| **50 MiB** read-ahead 16 (after) | 201 | 52 428 800 | 1.00x | **4 194 096** | **77.71 ms** |

Cancel at 50 % of 50 MiB — bytes served past what the copy wrote:

| | wrote | served | discarded | latency |
|---|---:|---:|---:|---:|
| striping (before) | 26 373 120 | 27 261 624 | 888 504 | 58.79 ms |
| read-ahead 16 (after) | 26 474 220 | 30 407 196 | **3 932 976** | 48.06 ms |

Also: one `SSH_FXP_OPEN` per file instead of up to four; the after rows are
byte-exact where the striped rows carried the per-seek 1 011-byte remainder.
The larger cancel discard tracks the in-flight budget and is accepted — it is
bandwidth after a cancel, never a wrong byte in the file.

LOC in `crates/ssh` (+495 / -388 overall across five files):

| | before | after | delta |
|---|---:|---:|---:|
| `pipeline.rs` production (above `mod tests`) | 218 | 99 | **-119** |
| `download.rs` | 260 | 259 | -1 |
| `session.rs` (`sftp_config` comment) | | | +2 |
| **production total** | | | **-118** |
| `pipeline.rs` tests | 180 | 173 | -7 |
| `pipeline_budget_tests.rs` | 384 | 615 | +231 |
| `us0095_verify_tests.rs` (comments only) | | | +6 |
| **test total** | | | **+230** |

Deleting 119 lines of re-order buffer, `JoinSet` lifecycle and multi-handle
bookkeeping, and spending 230 on tests that assert the contract those lines used
to carry implicitly, is the trade this packet makes deliberately.

## Documentation

### Owning Docs Reviewed

- `docs/sftp-browser-design.md` — transfer queue, `TransferEvent` contract, cancel semantics,
  `.part` staging and atomic finalize. **Changed**: the transfer section described the striped
  multi-handle download.
- `docs/ssh-client-connect.md` — SSH/SFTP session setup and `open_sftp`. **No change**: it
  describes the channel/subsystem handshake and never mentions striping or the read budget.
- `docs/spec-intakes/IN-0036-russh-0-63/low-level-design/upgrade.md` — Change F/G measurements
  and the "if `copy_striped` is ever retired, raise `max_concurrent_reads` in the same change"
  note. **Changed**: the note is closed with a pointer here.
- `docs/spec-intakes/IN-0036-russh-0-63/evidence/US-0095-verify.md` — the D2 finding (striping and
  read-ahead fight) and the counting-server harness reused for this packet's measurements.
  **Reviewed, no change**: it is a dated evidence record of what US-0095 measured and stays as
  written.
- `docs/spec-intakes/IN-0025-sftp-dual-pane/` — checked for a transfer-queue contract that would
  constrain progress or cancellation. It defines the dual-pane layout and the per-backend queue
  *ownership*, not the byte loop or the event cadence. **No change.**
- `docs/agents/error-policy.md` — `AppError::Cancelled` must be distinguishable from a failure and
  reach the UI as a cancellation, not an error. Preserved: `copy_sequential` returns
  `AppError::Cancelled` and `report_cancellation` still emits `TransferEvent::Cancelled`.

### Documentation Action

Update required: `docs/sftp-browser-design.md` (transfer section) and the IN-0036 LLD's open note.
`docs/ssh-client-connect.md`, the US-0095 evidence record and the IN-0025 intake are reviewed with
an explicit no-change reason above.

### Reconciliation

Docs changed: `docs/sftp-browser-design.md`,
`docs/spec-intakes/IN-0036-russh-0-63/low-level-design/upgrade.md`. The no-change reasons recorded
above remain valid.

## Context

`transfer::pipeline` is `pub(super)` inside `crates/ssh/src/sftp_task/`; `download.rs` is the only
caller of the striped path, `upload.rs` the only other caller of `copy_sequential`. The library
behaviour this depends on is in `russh-sftp-3.0.0/src/client/fs/file.rs` (`ReadState::request`,
`poll_read`, `Drop for File`) and `client/rawsession.rs` (`Drop for Request`, `SessionInner::reply`);
the HLD cites the line ranges.

## Plan

- [x] Baseline both paths on the same 5 MiB and 50 MiB files with the US-0095 counting server.
- [x] Measure cancel latency (bytes served after the cancel) and the no-resume cancel contract.
- [x] Replace the striped call site with `copy_sequential` over `reader.take(total)`.
- [x] Threshold the progress cadence on bytes copied.
- [x] Delete the striped path and its tests.
- [x] `max_concurrent_reads: 16`.
- [x] Rewrite `pipeline_budget_tests` around the new contract; invert the D2 regression case.
- [x] New tests: cancel-within-one-chunk, shrinking file, zero-length, exactly-one-packet,
  over-long file, progress-event count.
- [x] Docs + evidence.

## Decisions

No `DEC-` record. The architectural choice (library pipelining over hand-rolled striping) was
already decided by `US-0095`'s Change G measurement table and the owner's instruction; this packet
executes it and the HLD carries the rationale.

## Verification Plan

- Focused: `cargo test -p oneterm-ssh sftp_task::transfer::pipeline`
- Unit/budget: `cargo test -p oneterm-ssh pipeline_budget` (measurement tables printed with
  `-- --nocapture`)
- Regression: `cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools`
- Gate: `cargo fmt --all` then `pwsh scripts/ci-local.ps1 --full`

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Full tables, the library-source findings behind the cancellation claim, the test inventory and
the `harness.db` rows are in
[`evidence/US-0096-measurements.md`](evidence/US-0096-measurements.md).

Commands run:

| Command | Result |
|---|---|
| `cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools` | **91 / 49 / 16** (baseline 86 / 49 / 16; -5 striping tests, +10 new) |
| `cargo fmt --all` | clean |
| `pwsh scripts/ci-local.ps1 -Full` | pass |

### Gaps

- **No real-host run.** Everything is an in-process duplex against an in-process SFTP server:
  no TCP, no SSH encryption, no RTT, no OpenSSH `limits@openssh.com` reply. Correctness is as
  strong as the harness; throughput is not measured end to end.
- **The RTT benefit is extrapolated.** It follows from `throughput = in-flight bytes / RTT`, a
  measured 4.0x in-flight ratio and an identical request count. The only wall-clock evidence is
  the 50 MiB loopback run (28 % faster), which is a floor, not the expected figure on a real link.
- **No GUI walk.** `crates/sftp-ui` has no code change and its 49 tests pass unchanged, but the
  transfer queue was not exercised in the running app.
- **`max_concurrent_writes` stays at 8** against russh-sftp 3.0's 16 — `US-0095`'s decision,
  deliberately not revisited here. Available as separate measurable work.
- **Resume is still unimplemented** — not a regression, it never existed. The sequential reader
  makes it a one-`seek` change if the owner wants it. Owner decision.

## Handoff

None — the packet is complete in one session.
