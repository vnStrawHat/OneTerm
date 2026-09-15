# Independent verification: US-0096 (IN-0037)

Packet: [`../US-0096-library-read-ahead.md`](../US-0096-library-read-ahead.md)
Implementer evidence: [`US-0096-measurements.md`](US-0096-measurements.md)
Branch verified: `refactor/sftp-library-read-ahead` @ `74456d2` (4 commits over `main` @ `d214a10`)
Verifier: independent session, separate worktree, did not write the implementation
Host: Windows 11, debug build, in-process duplex transport (no network)
Date: 2026-09-15

## Verdict

**PASS-WITH-NOTES.**

Every load-bearing behavioural claim reproduced independently, several of them with a
measurement the implementer did not take (wire-level in-flight depth, transport death
mid-copy, destination failure mid-copy, `.part` cleanup through the real `sftp_download`
entry point). No defect was found in shipped behaviour.

Three defects are records and comments, not code behaviour: one stale doc comment on the very
function the change edits, one stale sentence in a required-reading agents doc that the packet
never listed as reviewed, and the absent `harness.db` rows. All three are cheap to close.

## What was checked, and how

| # | Claim | Result |
|---|---|---|
| 1 | `copy_striped` / `read_handles_for` / `REORDER_WINDOW` / `READ_PIPELINE_DEPTH` gone, no other caller | **Confirmed** |
| 2 | 5 MiB and 50 MiB tables, 21 / 201 READs, 1.00x, in-flight 4 194 096 vs 1 044 480 | **Confirmed**, and in-flight measured on the wire rather than derived |
| 3 | Cancel stops within one chunk, `.part` removed, session usable afterwards | **Confirmed** (stops on the *next* loop turn, 0 bytes past); session death and destination failure additionally proved to error rather than hang |
| 4 | Progress cadence matches what `crates/sftp-ui` expects | **Confirmed** |
| 5 | Shorter / longer than announced, zero-length, one packet | **Confirmed**; see N9 for the pre-existing silent-truncation semantics |
| 6 | russh-sftp 3.0 drop behaviour, no request-id confusion | **Confirmed from source** |
| 7 | Uploads unchanged, write budget still 262 144 / 8 | **Confirmed** (see N5 for one sub-claim that is inferred, not measured) |
| 8 | Harness records, owning docs, `check-english` / `check-doc-paths` | **Partly** -- D2, D3 below |
| 9 | `pwsh scripts/ci-local.ps1 -Full` | **Confirmed** -- see Commands |

## Findings

### D1 -- Medium-low. `sftp_config()`'s own doc comment still describes the deleted striping

`crates/ssh/src/session.rs:493-499`:

```
/// The russh-sftp client configuration, pinned to the transfer budget
/// russh-sftp 2.3.0 gave OneTerm (`IN-0036`, `US-0095` Changes F and G).
///
/// 3.0's defaults are not a drop-in: they cut the in-flight write budget 4x and
/// add a read-ahead that OneTerm's own striped download throws away. Every field
/// below restores measured 2.3.0 behaviour; none of them raises a limit past
/// what the server allows.
```

Two sentences are now false, and both are contradicted by the field comment 20 lines below in
the same function:

- "a read-ahead that OneTerm's own striped download throws away" -- there is no striped
  download; `IN-0037` deleted it, and the whole point of the change is that OneTerm now *uses*
  that read-ahead.
- "Every field below restores measured 2.3.0 behaviour" -- `max_concurrent_reads: 16` is
  russh-sftp **3.0's** default. 2.3.0 had no read-ahead at all; restoring 2.3.0 here would mean
  `1`, which the branch's own budget test now treats as the regression case.

`docs/agents/code-style.md` ("Remove outdated comments when changing code") makes this a rule
violation, not a nit. It is also exactly the class of staleness `US-0095`'s verification called
out as finding D3 last time.

Reproduction: `sed -n '493,530p' crates/ssh/src/session.rs` -- read the function header against
the `max_concurrent_reads` comment inside it.

Suggested fix: replace the paragraph with "3.0's write defaults are not a drop-in and are pinned
back to 2.3.0's budget (Change F); its read-ahead is now OneTerm's only download pipeline
(`IN-0037`)."

### D2 -- Medium-low. A required-reading agents doc still describes the striping, and was never listed as reviewed

`docs/agents/dependencies.md:65`:

> The SFTP client is constructed with `session::sftp_config()`, not russh-sftp's defaults: 3.0
> cut the in-flight write budget 4x and **added a read-ahead OneTerm's striped download
> discards**.

Present tense, and now wrong. Two reasons this is more than a typo:

1. `AGENTS.md` section 1 lists `docs/agents/dependencies.md` as required reading for every agent
   session, and this row is the dependency-policy contract that names `sftp_config()`. The next
   agent to read it will be told the opposite of what the code does.
2. The packet's "Owning Docs Reviewed" list does not contain it, so this is a **documentation
   record gap** under `docs/HARNESS.md`'s Completion Contract clause 4, not only a stale line.
   The packet's own grep-able criterion ("`copy_striped` ... absent from the tree") passed while
   the *claim* it encodes survived in two places.

Reproduction: `grep -rn "striped download" docs/ crates/ --include=*.md --include=*.rs`
(only `session.rs:497` and `dependencies.md:65` are live prose; every other hit is correctly
past-tense or an `IN-0036` dated record).

### D3 -- Low. `harness.db` has no `IN-0037` intake row and no `US-0096` story row

The database at the repository root (`D:\TrungKFC-Research\Rust\myTerm2\harness.db`; there is no
worktree-local copy) stops at:

```
intake  id=41  document_number=36  IN-0036  story_id=US-0095
story   US-0095  status=implemented
```

Nothing for `IN-0037` / `US-0096`, yet the packet's `HARNESS:STATUS` block is ticked
`Implemented` and every `HARNESS:PROOF` box except E2E is ticked. `docs/HARNESS.md` section Source
Ownership makes `harness.db` canonical for "Work status, proof result and evidence summary ...
intake", and says the CLI mirrors those fields *into* the packet -- so right now the mirror
exists without its source.

Mitigating: the implementer disclosed this explicitly in
`evidence/US-0096-measurements.md` section "Harness rows" ("`harness.db` was not edited from this
worktree") and supplied the exact `INSERT` statements, including `document_number = 37`,
`input_type = "change_request"`, `risk_lane = "normal"`, which match the Markdown intake header.
This is a mechanical step outstanding at merge, not a misrepresentation.

Reproduction:

```
python - <<'EOF'
import sqlite3
c = sqlite3.connect("harness.db")           # repo root, copy it first
print([dict(zip([d[1] for d in c.execute("pragma table_info(intake)")], r))
       for r in c.execute("select * from intake order by id desc limit 1")])
print([r[0] for r in c.execute("select id from story order by rowid desc limit 1")])
EOF
```

### N4 -- Note. The HLD describes a size-less branch that was not written

`high-level-design.md` "Data Flow" step 4 and the "Shrinking file, EOF" section say
`download_file_contents` calls `copy_sequential(&mut reader.take(total), ...)` for a sized file
"and `copy_sequential(&mut reader, ...)` for a size-less one", and that a zero-length file "takes
the size-less branch". There is no branch: `crates/ssh/src/sftp_task/transfer/download.rs:97`
sets `announced = if total > 0 { total } else { u64::MAX }` and always wraps in `.take()`.

The behaviour is identical (`take(u64::MAX)` never fires) and the shipped code is the simpler of
the two, so this is a design doc that describes a slightly different implementation than the one
that shipped. Worth one sentence.

### N5 -- Note. "Uploads emit the byte-identical progress sequence" is inferred, not measured

The upload *bytes* and *budget* are unambiguously untouched, and I verified that independently:
`git diff main..HEAD` shows **no change at all** to
`crates/ssh/src/sftp_task/transfer/upload.rs`, `.../staging.rs`, `.../transfer.rs`, or anything under
`crates/sftp-ui/`, and the Change F test still reports
`21 WRITE packets (largest 261120 B); in-flight budget 2088960 B` with
`max_write_packet_len: 262_144` / `max_concurrent_writes: 8` pinned in `sftp_config()`.

But uploads share `copy_sequential`, whose progress cadence **did** change from "once per read"
to "once per `CHUNK_LEN` of bytes, plus a tail". The sequences coincide only while the local
`tokio::fs::File::read` fills the whole 261 120-byte buffer on every call, which `AsyncRead` does
not guarantee (a short read from any source collapses two former samples into one). Nothing in
the suite pins an upload's sample sequence.

Impact is cosmetic: `transfer::send_progress` uses `try_send` on a bounded channel and drops
samples by design, and `sftp-ui::run_transfer` stores only the latest fraction. So this is not a
defect -- only a claim stated more strongly than the evidence supports.

### N6 -- Note. "In flight" in the packet table is arithmetic; I measured it instead

The implementer's evidence says so plainly ("**'In flight' is a budget, not a measurement**"),
and the shipped test computes `sftp_config().max_concurrent_reads * READ_PACKET_LEN` rather than
observing the wire. I wrapped the client half of the duplex in a packet-type sniffer that counts
outgoing `SSH_FXP_READ` (type 5) against incoming `SSH_FXP_DATA`/`SSH_FXP_STATUS` (103/101) and
tracks the running maximum, with a 3 ms server-side read delay so the queue can actually fill:

```
V1 wire: shipped max in-flight READs 16 (4194096 B), READs sent 36, server READs 21,
         served 5242880 B (1.00x), OPENs 1; depth-1 arm max in-flight 1
```

So the 4 194 096 B figure is true as measured, not only as configured, and the depth-1 regression
arm really is one request at a time.

### N7 -- Note. The "before" rows cannot be re-measured from this branch

Both tables' striped rows came from `in0037_baseline_tests.rs`, which the packet says was deleted
after the run, and the striped code itself is gone. I could not reproduce them. What I could do:

- re-derive `1 044 480 B = READ_PIPELINE_DEPTH (4) x CHUNK_LEN (261 120)` from the deleted code as
  it appears in the diff, and confirm `read_handles_for(5 MiB)` returns 4 (21 chunks / 4 = 5,
  clamped to 4);
- confirm the striped over-read arithmetic: 5 263 100 - 5 242 880 = 20 220 = 20 seeks x 1 011 B,
  and 52 631 000 - 52 428 800 = 202 200 = 200 x 1 011 B. Both consistent.

Every "after" row I reproduced exactly (below).

### N8 -- Note. The cancel-discard figure is timing-dependent; my number differs

The packet reports **3 932 976 B** discarded when cancelling at 50 % of 50 MiB. Re-running the
same construction I measured **3 146 583 B**. Both sit under the 4 194 096 B budget, and the
difference is how much of the read-ahead had already drained into `ReadState.buffer` when the
token fired. The claim ("tracks the in-flight budget, bandwidth not correctness") holds; the
exact number should not be quoted as reproducible.

### N9 -- Note. A file that shrank after `stat` is finalized as a success

Driving the real `sftp_download` against a server whose `lstat` announces three chunks while the
file holds one and a bit:

```
V7 shrank: announced 783360 B, landed 261130 B, result ok = true, last progress 1
```

The local file is replaced with the short content, `TransferEvent::Progress(1.0)` is emitted, and
`sftp-ui` settles the queue item `Completed`. Nothing distinguishes it from a complete download.

This is **not a regression**: the retired `copy_striped` clamped `chunk_count` and "ended early
without error" in the same way, the HLD's table states it, and `docs/sftp-browser-design.md` now
documents it. I flag it because `IN-0037`'s lane argument is built on "the data-loss trigger does
not fire", and this is the one place where a user can be told a transfer succeeded while holding
fewer bytes than the server advertised. Cheap hardening if the owner wants it: compare
`copied` against `total` after `copy_sequential` and fail, since `finalize_local_file` has not
run yet at that point.

### N10 -- Note. The read-ahead over-requests past EOF; no table counts it

My wire sniffer shows the client putting **36** `SSH_FXP_READ` packets out for a 5 MiB file, of
which 21 return data -- `ReadState::request` tops the queue back to 16 on the read that consumes
the last data, so 15 requests land past EOF and are answered `SSH_FX_EOF`. Bandwidth is
negligible (request headers plus status replies, all pipelined, no extra latency), and the same
15 would be issued by any consumer of the library. Worth knowing that "one READ per read packet"
counts server-side *answered* reads: the shipped test asserts the server's counter, not the
wire's.

### N11 -- Note. `risk_lane: normal` is defensible; I verified its premises rather than its conclusion

`IN-0037.md` justifies the normal lane on three checkable claims. All three hold:

- **no resume feature**: `grep -rni "resume" crates/sftp-ui/src crates/ssh/src` returns nothing
  (I ran it; exit 1, no hits);
- **`finalize_local_file` untouched**: `crates/ssh/src/sftp_task/transfer/staging.rs` is
  byte-identical in `git diff main..HEAD`;
- **upload untouched**: likewise `upload.rs`, and `crates/sftp-ui/` has no diff at all.

I would still have argued for the high-risk lane on `docs/HARNESS.md`'s "data loss ... or
integrity" and "broad established behavior" triggers -- this is the byte path of every file the
user downloads -- and that would have required a detail design under
`IN-0037-sftp-library-read-ahead/low-level-design/` before `story create`, which does not exist.
In practice the HLD carries what an LLD would (library line references, cadence arithmetic, the
three EOF cases), and I checked each of those references against the crate source myself. Not a
blocker; recorded so the lane choice is visible rather than assumed.

## My measurements

All figures re-measured in my own worktree with my own tests (see "Test files" below), against an
in-process `russh_sftp::server::Handler` over `tokio::io::duplex`, `sftp_config()` as shipped.

### Downloads, shipped configuration

| | server READs | bytes served | ratio | max in-flight READs (wire) | in-flight bytes | `SSH_FXP_OPEN` | progress samples | wall |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 5 MiB | 21 | 5 242 880 | **1.00x** | **16** | 4 194 096 | **1** | 21 | 24.6 ms |
| 50 MiB | 201 | 52 428 800 | **1.00x** | 16 | 4 194 096 | **1** | 201 | 103.9 ms |
| 5 MiB, `max_concurrent_reads: 1` | 21 | 5 242 880 | 1.00x | **1** | 262 131 | 1 | -- | 17.3 ms |

`21 = 5 242 880 / 262 131` rounded up, `201 = 52 428 800 / 262 131` rounded up. Progress samples
are `size / CHUNK_LEN` rounded up (21 and 201). Every one of these matches the packet.

Re-running the branch's own suite reproduces its printed tables verbatim:

```
IN-0037: 5 MiB download, 5242880 bytes wanted
  shipped (read-ahead 16): 21 READs, 5242880 B (1.00x), 4194096 B in flight, 24.591ms
  regression (read-ahead 1): 21 READs, 262131 B in flight, 17.3234ms
  retired striping         : 21 READs, 1044480 B in flight
IN-0037 D2: seek-per-chunk against the shipped read-ahead: 201 READs, 48700595 B (9.29x the file), 83.4077ms
US-0095 F: 5 MiB upload in 21 WRITE packets (largest 261120 B); in-flight budget 2088960 B vs russh-sftp 2.3.0's 2088960 B
```

The 9.29x D2 figure matches `US-0095`'s and the inverted guard fires the right way round.

### Cancellation and failure paths

| Scenario | Result |
|---|---|
| Cancel at 50 % of 50 MiB | wrote 26 474 220 B = exactly the byte count at which the token fired; **0 bytes written past the cancel**; 3 146 583 B served and discarded (budget 4 194 096 B); a second full 50 MiB download on the **same session** is byte-identical |
| Cancel through the real `sftp_download` (20 MiB, cancel at 25 %) | `Err(AppError::Cancelled)`, `TransferEvent::Cancelled` emitted, destination directory **empty** -- no `.part` sibling, no target file |
| Transport dies mid-copy (client stream reports EOF after 2 MiB) | `Err("read: session closed")` within the 20 s guard -- an error, not a cancellation, not a hang |
| Destination write fails mid-copy (`FailingSink` after 3 chunks) | `Err("write: no space left on device")` promptly; dropping the reader with the read-ahead in flight leaves the session usable and a subsequent 10 MiB download is byte-identical |

The "stops within one chunk" claim is conservative: because the token is observed by a `biased`
`select!` at the top of the loop, the copy stops on the **next turn**, so zero bytes are written
after the cancel, not up to one chunk.

### Length edge cases (through the real `sftp_download`)

| Case | Destination | Result | Last progress |
|---|---|---|---|
| announced 783 360 B, server has 261 130 B | 261 130 B, correct prefix | `Ok(())` | 1.0 -- see N9 |
| announced 261 127 B, server has 783 360 B | 261 127 B, correct prefix, no over-read into the file | `Ok(())` | 1.0 |
| zero-length | empty | `Ok(())` | 1.0 from `sftp_download`, and `run_transfer` also forces `progress = 1.0` on `Ok` -- so the UI never depends on a pipeline sample here, which is just as well because `copy_sequential` correctly emits **none** |
| exactly one read packet (262 131 B) | byte-identical | `Ok(())` | 1 sample |

No infinite loop in any of them; every case terminates on `Take`'s limit or on EOF.

### Progress consumer, checked against the new cadence

`crates/sftp-ui/src/transfer.rs:88-101` stores only the latest fraction
(`item.progress = progress`) and `crates/ssh/src/sftp_task/transfer.rs:33-41` drops a sample when
the bounded channel is full by design, so there is **no minimum cadence requirement** to break.
Terminal state does not depend on a pipeline sample either: `sftp_download` appends
`Progress(1.0)` unconditionally and `run_transfer` sets `progress = 1.0` on `Ok`.

One consumer does accumulate rather than overwrite --
`download.rs:225-240`'s directory closure does `bytes_done = file_start + done`. I checked it
against the thresholded cadence: `copy_sequential` always emits a final sample equal to the real
byte total whenever anything was copied, so `bytes_done` is exact after each file, and a
zero-length file (no sample at all) contributes zero bytes, so it cannot drift either. No defect.

### russh-sftp 3.0.0 drop semantics, read from the crate source

`~/.cargo/registry/src/index.crates.io-*/russh-sftp-3.0.0/src/`:

| Claim | Verified at | Verdict |
|---|---|---|
| `Drop for Request` de-registers the response slot | `client/rawsession.rs:63-69` -- `if !self.completed { self.requests.remove(&self.id) }` | **True** |
| A reply for a dropped request is ignored, not an error | `client/rawsession.rs:77-103` -- unknown `Some(id)` logs `debug!("ignoring reply for completed or cancelled request")` and returns `Ok(())` | **True** |
| `Drop for File` closes without awaiting | `client/fs/file.rs:272-280` -- `close_nowait`, return value discarded | **True** |
| Seek throws the read-ahead away | `client/fs/file.rs:41-47, 343-355` -- `poll_complete` calls `ReadState::reset`, which clears `pending` locally | **True** |
| The queue really is `max_concurrent_reads` deep, after a 1-request probe | `client/fs/file.rs:103-135` -- `count = if self.chunk_len.is_some() { max_concurrent_reads } else { 1 }` | **True** (and measured, N6) |

**Request-id reuse: not a practical hazard.** Ids come from
`next_req_id: AtomicU32::new(1)` with `fetch_add(1)` (`rawsession.rs:174, 221, 274-276`) -- strictly
monotonic, never recycled until a u32 wrap. A late reply can therefore only ever find *no* map
entry, never a different request's, and `SessionInner::reply` turns that into a `debug!` line.
Wraparound would need ~4.29e9 requests on one session (~1 PB at 262 131 B each). Proved
behaviourally too: after a mid-transfer cancel and after a destination failure, the next full
download on the same session is byte-identical in my V3 and V5.

One related library behaviour worth recording, unchanged by this packet: `ReadState` pins
`chunk_len` from the **first** response (`file.rs:76`) for the life of the handle, so a server
that answers the first READ short would shrink every later request on that handle. It behaved
this way before `IN-0037` too and no OneTerm code influences it.

### Diff hygiene

- `grep -rn "copy_striped\|read_handles_for\|REORDER_WINDOW\|READ_PIPELINE_DEPTH" crates/ docs/ scripts/`
  finds **no definition and no caller** -- every remaining hit is a past-tense comment, a dated
  `IN-0036` record, or the new intake's own prose.
- Production LOC in `pipeline.rs` above `mod tests`: 218 -> 99 (**-119**); with `session.rs` +2 and
  `download.rs` -1 that is the claimed **-118**. Confirmed by line count on both revisions.
- Files with no diff at all: `upload.rs`, `staging.rs`, `sftp_task/transfer.rs`, all of
  `crates/sftp-ui/`.

## Commands run

| Command | Result |
|---|---|
| `git reset --hard refactor/sftp-library-read-ahead` | `HEAD` = `74456d2` |
| `git log --oneline main..HEAD` | 4 commits (`d823980`, `0a7d869`, `a103baf`, `74456d2`) |
| `git diff main..HEAD -- crates/ docs/` | read in full; 11 files, +1348 / -406 |
| `cargo test -p oneterm-ssh --lib -- --nocapture sftp_task::transfer::pipeline` | **13 passed, 0 failed** (tables above) |
| `cargo test -p oneterm-ssh --lib -- --nocapture --test-threads=1 in0037_verify` | **8 passed, 0 failed** (my own tests) |
| `python scripts/check-english.py` | passed, 806 files |
| `python scripts/check-doc-paths.py` | passed, 190 paths in 11 documents |
| `pwsh scripts/ci-local.ps1 -Full` (`CARGO_BUILD_JOBS=4`, pristine tree, my test files moved aside) | **exit 0, "ci-local: all checks passed"**; summed over every test-result line: **60 sections / 2002 passed / 0 failed / 14 ignored**; `advisories ok, bans ok, licenses ok`. Exactly the claimed 60/2002/0/14. Run twice, same result. |
| per-crate counts inside that run | `oneterm-ssh` **91**, `oneterm-sftp-ui` **49**, `oneterm-tools` 14 lib + 2 integration = **16** -- the packet's 91 / 49 / 16 |

## What I could not verify

- **Anything over a real link.** No TCP, no SSH encryption, no RTT, no OpenSSH
  `limits@openssh.com` reply. The throughput conclusion ("4.0x in-flight is 4.0x the speed on a
  latent link") remains an extrapolation from `throughput = in-flight / RTT`; the only wall-clock
  evidence on either side is loopback, where the two are within noise. The packet discloses this.
- **The "before" rows** -- the striped arm and its baseline test are both deleted (N7).
- **The running app.** No GUI walk. The owner runs Claude Code inside their own `oneterm.exe`, so
  no `oneterm` process was started, inspected, or stopped by this verification.
- **A server that answers the first READ short** -- the `chunk_len` pinning noted above. Not
  reachable with the in-process server, and unchanged by this packet either way.
- **`harness.db` after merge** -- I read a copy of the root database and did not modify it.

## Test files written (verifier-owned, not committed)

`crates/ssh/src/sftp_task/transfer/in0037_verify_tests.rs`, registered from
`crates/ssh/src/sftp_task/transfer.rs` with

```rust
#[cfg(test)]
#[path = "transfer/in0037_verify_tests.rs"]
mod in0037_verify_tests;
```

Eight tests, all passing, written from the outside and worth adopting -- V4, V5, V6 and V8 cover
paths the shipped suite does not exercise at all:

| Test | What it pins |
|---|---|
| `v1_measured_wire_depth_is_the_configured_read_ahead` | in-flight READ depth **measured on the wire** (16, and 1 in the depth-1 arm), one `SSH_FXP_OPEN`, 1.00x |
| `v2_fifty_mib_table` | the 50 MiB row: 201 READs, 1.00x, 201 progress samples |
| `v3_cancel_at_half_of_fifty_mib` | zero bytes written past the cancel, discard within budget, session survives |
| `v4_transport_death_mid_copy_is_an_error_not_a_hang` | a dead SFTP transport errors instead of hanging, and is not mistaken for a cancellation |
| `v5_destination_write_failure_propagates_without_hanging` | a full disk mid-copy errors promptly and leaves the session reusable |
| `v6_cancelled_download_leaves_no_part_file_and_no_target` | the real `sftp_download`: `Cancelled` event and an **empty** destination directory |
| `v7_a_file_that_shrank_after_stat_is_reported_as_a_success` | documents N9's semantics so a future change cannot alter them silently |
| `v8_a_file_that_grew_after_stat_is_silently_truncated_to_the_announced_size` | the `take` cap through the real entry point |

It also carries two reusable pieces the branch's harness lacks: a `WireSniffer` that counts SFTP
packet types on either half of the duplex, and a `DyingStream` that turns the transport off
mid-transfer.

## Recommendation

Merge after D1 and D2 (two comment edits) and D3 (the `INSERT`s the implementer already wrote
out). N4 is one sentence in the HLD. N5 and N9 are worth a line in the packet's Gaps; N9 is worth
an owner decision on whether a short download should fail rather than report success.
