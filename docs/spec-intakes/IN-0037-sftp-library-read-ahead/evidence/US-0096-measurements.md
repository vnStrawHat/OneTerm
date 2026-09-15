# Evidence: US-0096 — striped download vs the library's read-ahead

Intake: IN-0037
Packet: [`../US-0096-library-read-ahead.md`](../US-0096-library-read-ahead.md)
Date: 2026-09-15
Host: Windows 11, debug build, in-process duplex transport (no network)

## Harness

The counting SFTP server `US-0095` built for Change F/G
(`crates/ssh/src/sftp_task/transfer/pipeline_budget_tests.rs`): a
`russh_sftp::server::Handler` over one real file, connected to a real
`russh_sftp::client::SftpSession` through `tokio::io::duplex`, counting every
`SSH_FXP_READ` / `SSH_FXP_WRITE` and the bytes each carried. The duplex buffer was
raised from 1 MiB to 8 MiB so a 16-deep read-ahead can actually fill without the
pipe itself becoming the limit.

**"In flight" is a budget, not a measurement.** It is what the configuration allows
on the wire (`max_concurrent_reads x read packet length`, or the retired striping's
`handles x CHUNK_LEN`). An in-process duplex has no RTT, so the wall-clock column
below understates the difference by design — it is the in-flight column that
predicts a real link, where throughput is `in-flight bytes / RTT`.

## Before / after

Both arms measured in the same process, on the same file, back to back
(temporary `in0037_baseline_tests.rs`, deleted after the numbers were taken; the
striped arm called the real `copy_striped` before it was removed).

### 5 MiB download

| path | READs | bytes served | ratio | in flight | wall |
|---|---:|---:|---:|---:|---:|
| striping — 4 handles, read-ahead 1 (before) | 21 | 5 263 100 | 1.00x | 1 044 480 | 10.26 ms |
| library read-ahead — 1 handle, depth 16 (after) | 21 | **5 242 880** | **1.00x** | **4 194 096** | **9.66 ms** |

### 50 MiB download

| path | READs | bytes served | ratio | in flight | wall |
|---|---:|---:|---:|---:|---:|
| striping — 4 handles, read-ahead 1 (before) | 201 | 52 631 000 | 1.00x | 1 044 480 | 107.56 ms |
| library read-ahead — 1 handle, depth 16 (after) | 201 | **52 428 800** | **1.00x** | **4 194 096** | **77.71 ms** |

Reading:

- **Same request count.** 21 and 201 either way — the retired striping was not
  buying fewer round trips.
- **The after row is byte-exact.** 5 242 880 B for a 5 242 880 B file. The striped
  row's extra 20 220 B (and 202 200 B at 50 MiB) is the per-seek remainder
  `US-0095` documented: russh-sftp asks for 262 131 B while OneTerm consumed
  261 120, and each seek dropped the 1 011-byte difference. With no seeks there is
  no remainder and no slack to allow for, which is why the new assertion is
  `served == SIZE` exactly rather than `<= SIZE + slack`.
- **4.0x the in-flight budget** (4 194 096 vs 1 044 480). On loopback that is worth
  6 % and 28 %; on a link with RTT it is the ratio itself.
- **One `SSH_FXP_OPEN` instead of four** on every file — not in the table, but it is
  three saved round trips per file, which dominates a directory of small files.

### Cancel at 50 % of a 50 MiB download

| path | wrote before stopping | server served | discarded after cancel | latency |
|---|---:|---:|---:|---:|
| striping — 4 handles, read-ahead 1 | 26 373 120 | 27 261 624 | 888 504 | 58.79 ms |
| library read-ahead — 1 handle, depth 16 | 26 474 220 | 30 407 196 | **3 932 976** | 48.06 ms |

The copy stops within one chunk either way — that is what the cancel token
controls. What grows is the bytes already on the wire when it stops: ~3.75 MB
against ~0.85 MB, tracking the in-flight budget almost exactly. **This is a real
cost and it is accepted**: it is the same budget that buys the throughput, it is
bandwidth rather than correctness (the bytes are discarded by the dropped
`Request` and never reach the `.part` file), and it is paid only when a user
cancels.

The shipped regression test cancels a 5 MiB download at 50 % and prints the same
figure (1 573 797 B discarded of a 4 194 096 B budget — smaller because the file
ends before the pipeline is fully warm).

### Uploads — unchanged, and proved so

`an_upload_keeps_the_2_3_0_write_budget_in_flight` is untouched by this packet and
still reports:

```
US-0095 F: 5 MiB upload in 21 WRITE packets (largest 261120 B);
           in-flight budget 2088960 B vs russh-sftp 2.3.0's 2088960 B
```

`max_write_packet_len: 262_144` and `max_concurrent_writes: 8` are exactly as
`US-0095` Change F pinned them.

### The D2 finding, kept as a live guard

With the read-ahead now at 16, seeking per chunk is *more* expensive than ever.
The measurement is retained as
`pipeline_budget_tests::seeking_per_chunk_still_throws_the_read_ahead_away`:

```
IN-0037 D2: seek-per-chunk against the shipped read-ahead: 201 READs,
            48700595 B (9.29x the file), 72.01ms
```

This is why the striping had to be the thing that was deleted, and it fails the
build if anyone puts a seek back into the download loop.

### The regression case, inverted

`US-0095` shipped `max_concurrent_reads: 1` because striping supplied the
concurrency. With striping gone, depth 1 *is* the regression:

```
IN-0037: 5 MiB download, 5242880 bytes wanted
  shipped (read-ahead 16):   21 READs, 5242880 B (1.00x), 4194096 B in flight, 19.82ms
  regression (read-ahead 1): 21 READs,                     262131 B in flight, 11.30ms
  retired striping         : 21 READs,                    1044480 B in flight
```

(The depth-1 wall time is lower only because an in-process duplex rewards doing
less bookkeeping; with any RTT the 16x smaller in-flight budget is 16x the time.)
`a_download_costs_one_read_per_packet_and_keeps_the_read_ahead_in_flight` asserts
`sftp_config().max_concurrent_reads >= 16` and that the in-flight budget beats the
retired striping's, so dropping back to 1 fails the build.

## Cancellation teardown — read from the library, then tested

`copy_striped` called `JoinSet::abort_all`. The sequential path just drops the
reader, so the question is what russh-sftp 3.0 does with reads still queued.
From `~/.cargo/registry/src/*/russh-sftp-3.0.0/src/client/`:

| Concern | Answer | Source |
|---|---|---|
| Are pending response slots leaked? | No — `Drop for Request` removes its id from the shared `requests` map when the request did not complete | `rawsession.rs:62-67` |
| What happens to a reply that arrives afterwards? | `SessionInner::reply` finds no entry, logs `debug!("ignoring reply for completed or cancelled request")` and returns `Ok(())`. The session task neither errors nor stalls | `rawsession.rs:92-97` |
| Does dropping the `File` block? | No — `Drop for File` calls `close_nowait` and does not await the reply | `fs/file.rs:272-280` |

Tested rather than trusted:
`a_cancelled_download_stops_within_one_chunk_and_the_session_survives` cancels
mid-download and then runs a **second, complete** download on the same
`SftpSession`, asserting it is byte-identical. A leaked or wedged request map
would fail that.

## Test inventory

Baseline: `cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools`
= **86 / 49 / 16**. After: **91 / 49 / 16**.

### Removed (5, all `crates/ssh/src/sftp_task/transfer/pipeline.rs`)

| Test | Why it goes |
|---|---|
| `striped_copy_reassembles_chunks_in_order` | tested the re-order buffer, which no longer exists |
| `striped_copy_stops_when_the_file_is_shorter_than_announced` | replaced by `take_caps_a_long_file_and_a_short_one_ends_at_eof` and `a_file_shorter_than_announced_ends_at_eof_with_the_real_size` |
| `striped_copy_reads_only_the_announced_length` | replaced by the same two, plus `a_file_longer_than_announced_is_cut_at_the_announced_size` |
| `small_files_use_one_handle_and_large_files_ramp_up` | tested `read_handles_for`, deleted |
| `a_download_serves_exactly_the_file_and_both_candidates_are_measured` | the A/B fork it measured is now decided; split into the shipped-contract test and the D2 guard |

### Added (10)

`pipeline::tests` (4): `progress_cadence_survives_a_reader_that_splits_differently`,
`take_caps_a_long_file_and_a_short_one_ends_at_eof`,
`a_zero_length_file_copies_nothing_and_reports_nothing`, and
`cancelled_copies_report_cancelled` rewritten for the one remaining copy loop.

`pipeline_budget_tests` (6), all over a real `SftpSession`:
`a_download_costs_one_read_per_packet_and_keeps_the_read_ahead_in_flight`,
`seeking_per_chunk_still_throws_the_read_ahead_away`,
`a_cancelled_download_stops_within_one_chunk_and_the_session_survives`,
`a_file_shorter_than_announced_ends_at_eof_with_the_real_size`,
`a_file_longer_than_announced_is_cut_at_the_announced_size`,
`a_zero_length_file_and_a_one_packet_file_download_exactly`,
`a_download_reports_one_progress_sample_per_chunk`.

Each budget test drives the exact composition `download_file_contents` uses —
`sftp.open()`, `.take(announced)`, `copy_sequential` — so the assertions are about
the shipped download path, not about a stand-in.

## Gaps

- **No real-host run.** Everything above is an in-process duplex against an
  in-process server: no TCP, no SSH encryption, no RTT, no OpenSSH
  `limits@openssh.com` reply. The correctness claims (byte-exactness, EOF, cancel,
  cadence) are as strong as the harness; the **throughput** claim is not measured
  end to end.
- **The RTT benefit is extrapolated, not measured.** It follows from
  `throughput = in-flight bytes / RTT`, a measured 4.0x in-flight ratio, and a
  measured-identical request count — but the constant was not confirmed against a
  real link. The 50 MiB loopback run (77.7 ms vs 107.6 ms, 28 % faster) is the only
  wall-clock evidence and it is a floor, not the expected figure.
- **No GUI walk.** `crates/sftp-ui` has no code change and its 49 tests pass
  unchanged, but the transfer queue was not exercised in the running app.
- **`max_concurrent_writes` stays at 8.** russh-sftp 3.0 defaults to 16. `US-0095`
  decided that on its own evidence and this packet did not revisit it; doubling the
  upload in-flight budget remains available as separate, measurable work.
- **Resume is still unimplemented.** Not a regression — it never existed — but the
  sequential reader makes it a one-`seek` change if the owner wants it.

## Harness rows

`harness.db` was not edited from this worktree. The equivalent rows:

```python
import sqlite3, datetime
now = datetime.datetime.now().isoformat(timespec="seconds")
db = sqlite3.connect("harness.db")
db.execute(
    "INSERT INTO intake (created_at, input_type, summary, risk_lane, risk_flags, "
    "affected_docs, story_id, doc_path, notes, document_number, design_doc) "
    "VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    (
        now,
        "change_request",
        "Retire OneTerm's striped SFTP download in favour of russh-sftp 3.0's "
        "pipelined read-ahead; restore max_concurrent_reads to 16.",
        "normal",
        "throughput; transfer cancellation; progress cadence",
        "docs/sftp-browser-design.md; "
        "docs/spec-intakes/IN-0036-russh-0-63/low-level-design/upgrade.md",
        "US-0096",
        "docs/spec-intakes/IN-0037-sftp-library-read-ahead/IN-0037.md",
        "Executes the option B that US-0095 Change G measured but had no mandate "
        "to take. Lane is normal, not high_risk: OneTerm has no resume feature, "
        "so no offset arithmetic can corrupt a user's file.",
        37,
        "docs/spec-intakes/IN-0037-sftp-library-read-ahead/high-level-design.md",
    ),
)
intake_id = db.execute("SELECT last_insert_rowid()").fetchone()[0]
db.execute(
    "INSERT INTO story (id, title, created_at, risk_lane, contract_doc, packet_doc, "
    "status, unit_proof, integration_proof, e2e_proof, platform_proof, evidence, "
    "verify_command, last_verified_at, last_verified_result, notes, intake_id) "
    "VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    (
        "US-0096",
        "SFTP downloads read sequentially on russh-sftp's pipelined read-ahead",
        now,
        "normal",
        "docs/sftp-browser-design.md",
        "docs/spec-intakes/IN-0037-sftp-library-read-ahead/"
        "US-0096-library-read-ahead.md",
        "implemented",
        "pass: cargo test -p oneterm-ssh (91, was 86: -5 striping, +10 new)",
        "pass: pipeline_budget_tests over a real SftpSession -- 21 READs, "
        "5242880 B (1.00x), 4194096 B in flight vs striping's 1044480 B",
        "not run: no real SSH host; loopback duplex only",
        "pass: pwsh scripts/ci-local.ps1 --full",
        "docs/spec-intakes/IN-0037-sftp-library-read-ahead/evidence/"
        "US-0096-measurements.md",
        "pwsh scripts/ci-local.ps1 -Full",
        now,
        "pass",
        "Uploads unchanged (US-0095 Change F write budget re-asserted). Cancel "
        "discards up to the in-flight budget on the wire: ~3.9 MB vs striping's "
        "~0.9 MB at 50 MiB, accepted and documented.",
        intake_id,
    ),
)
db.commit()
```
