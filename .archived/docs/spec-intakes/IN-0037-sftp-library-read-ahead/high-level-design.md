# High-Level Design: SFTP downloads on the library's read-ahead

Intake: IN-0037
Lane: normal
Date: 2026-09-15

## Idea

OneTerm and russh-sftp both try to pipeline SFTP reads, and they cancel each other out.

russh-sftp 3.0's `client::fs::File` keeps up to `max_concurrent_reads` `SSH_FXP_READ` packets on
the wire and drains them into a local buffer as `AsyncRead` is polled. OneTerm's `copy_striped`
opens several handles onto the same file and *seeks* before every chunk — and `File::poll_seek`
calls `ReadState::reset`, which clears the **local** queue after the server has already served
those requests. `US-0095` measured the collision at 9.29x the file size on a 5 MiB download, and
stopped the bleeding the cheap way: `max_concurrent_reads: 1`, so OneTerm's striping is the only
pipeline left.

This change picks the other side of that fork. Delete the striping, read the file straight
through one handle, and let the library keep the pipe full. The wire cost is identical (1.00x,
one READ per packet-length chunk) but the in-flight budget is `max_concurrent_reads x
max_read_packet_len` = 16 x ~256 KiB ~ **4.2 MB** against striping's 4 handles x 255 KiB ~
**1.04 MB**, and SFTP throughput is in-flight bytes / RTT. On loopback the two are within noise;
on a 100 ms link the ratio is the speedup.

The second half of the idea is subtraction: `copy_striped`, `read_chunk`, `read_handles_for`,
`REORDER_WINDOW`, `READ_PIPELINE_DEPTH` and their tests are ~230 lines of re-order buffer,
`JoinSet` lifecycle and multi-handle bookkeeping that exist only to do what the library now does.

## Diagram

```text
BEFORE — OneTerm stripes, library read-ahead disabled (max_concurrent_reads: 1)

  download_file_contents
        |
        | sftp.open() x read_handles_for(total)   (1..4 handles, 1..4 OPEN round trips)
        v
  +--------------------------------------------------------------+
  |                        copy_striped                           |
  |                                                               |
  |  idle: [h0 h1 h2 h3]      JoinSet<(idx, handle, Result)>      |
  |     |                          ^         |                    |
  |     | spawn read_chunk(idx)    |         | join_next()        |
  |     |   seek(idx * 255K) ------+         v                    |
  |     |   read_exact(255K)            BTreeMap<idx, Vec<u8>>    |
  |     |                               (re-order window = 8)     |
  |     |                                     |                   |
  |     +-------------------------------------+ write in idx order|
  +--------------------------------------------------------------+
                                  |
                                  v                      in flight: 4 x 255 KiB = 1.04 MB
                          local .part file               per-handle library queue: 1

  Every seek() calls ReadState::reset. With max_concurrent_reads > 1 the requests the
  reset discards have already been served -> the 9.29x amplification US-0095 measured.


AFTER — one handle, library pipelines (max_concurrent_reads: 16)

  download_file_contents
        |
        | sftp.open()                              (1 OPEN round trip)
        v
  +--------------------------------------------------------------+
  |  russh_sftp File  (ReadState)                                 |
  |                                                               |
  |   pending: [READ@0 READ@256K READ@512K ... ] up to 16 deep    |
  |            issued by read_nowait, never seek-reset            |
  |   buffer:  the front response, drained by poll_read           |
  +--------------------------------------------------------------+
        |  AsyncRead
        v
  +--------------------------------------------------------------+
  |   copy_sequential(reader.take(total), local_file, cancel, on) |
  |     loop { select! { cancel -> Err(Cancelled),                |
  |                      read(&mut buf[..255K]) } ; write_all }   |
  +--------------------------------------------------------------+
        |
        v                              in flight: 16 x ~256 KiB = 4.2 MB
  local .part file
```

## UI Wireframe

`N/A — no UI surface.` The transfer queue row, its progress bar and its Cancel button are
untouched; `crates/sftp-ui` gains no code change. What reaches the UI is the same
`TransferEvent::Progress(f64)` / `TransferEvent::Cancelled` stream it consumed then. `BUG-0077`
later added `TransferEvent::Discovering(files_found)` for a folder download's listing pass; the
row shows it as `<name> (scanning, N files found)` until the first `Progress`:

```text
|  v  tree (scanning, 373 files found)          [----------]   0%   x |
|  v  tree                                      [###-------]  31%   x |
```

## Data Flow

1. `sftp_download` stats the remote path. A symlink is refused; a directory goes to
   `sftp_download_dir`, which calls `download_file_contents` per discovered file. Unchanged here;
   `BUG-0077` later split it into a listing pass and a download pass so folder progress has a
   true denominator.
2. `download_file_contents` opens **one** handle (`sftp.open`), where it previously opened
   `read_handles_for(total)` of them — up to four `SSH_FXP_OPEN` round trips saved on every file.
3. It creates the `.part` temporary sibling. Unchanged.
4. It calls `copy_sequential(&mut reader, &mut local_file, cancel, on_bytes)` over
   `reader.take(announced)`, where `announced = if total > 0 { total } else { u64::MAX }`. There
   is **one** call site, not a sized/size-less branch: `take(u64::MAX)` never fires, so the
   size-less case costs nothing and needs no second path. The cap is
   `tokio::io::AsyncReadExt::take` — the "read only the announced length" invariant
   `copy_striped` enforced by arithmetic, now enforced by the stdlib adapter.
5. Inside `copy_sequential`, each iteration polls `reader.read(&mut buffer[..CHUNK_LEN])`. The
   library serves it from its queued responses and tops the queue back up to
   `max_concurrent_reads`, so the wire stays full without OneTerm scheduling anything.
6. `write_all` to the `.part` file, then progress. On success `finalize_local_file` replaces the
   target and the remote mode/times are applied. Unchanged.

### What owns cancellation now

`copy_sequential`'s `tokio::select!` — the same construct that already guards uploads:

```rust
let read = tokio::select! {
    biased;
    _ = cancel.cancelled() => return Err(AppError::Cancelled),
    read = reader.read(&mut buffer) => read?,
};
```

`biased` makes the cancellation branch win a tie, so a token cancelled before the loop starts
returns without reading a byte. Otherwise cancellation is observed **within one read** — one
`CHUNK_LEN` chunk, the same bound the striped path had between `join_next()` calls.

The teardown replaces `JoinSet::abort_all`. Dropping the `File` is enough, and it neither leaks
nor blocks — verified against `russh-sftp-3.0.0/src/client/`:

- `ReadState.pending` holds each outstanding read as a `rawsession::Request`. `Drop for Request`
  removes its id from the shared `requests` map (`rawsession.rs:62-67`), so no oneshot sender is
  retained.
- A response that arrives for an id already removed is dropped with a `debug!` line —
  `SessionInner::reply` treats "reply for a completed or cancelled request" as `Ok(())`
  (`rawsession.rs:92-97`). The session task does not error out and does not stall.
- `Drop for File` calls `close_nowait` (`fs/file.rs:272-280`): the `SSH_FXP_CLOSE` goes out and
  the reply is never awaited, so dropping is synchronous and cannot block the cancelling task.
  **Superseded by `BUG-0076`:** that close never decrements russh-sftp's own open-handle count,
  which `limits@openssh.com` caps, so every dropped `File` leaked a unit until `open` failed with
  "handle limit reached". The download now awaits `File::close()` — inline on success, in a
  spawned task after a failed or cancelled copy so the cancel still does not block on the
  read-ahead draining ahead of the CLOSE reply.

**One honest cost.** Cancellation stops the *copy* within one chunk either way, but the bytes
already in flight still cross the wire before the server notices the closed handle: up to
`16 x 256 KiB ~ 4.2 MB` against striping's `4 x 255 KiB ~ 1.04 MB`. That is the same in-flight
budget that buys the throughput — it cannot be large for speed and small for cancellation. It
costs bandwidth after a cancel, never correctness: those bytes are discarded by a dropped
`Request` and never reach the `.part` file. Measured in the packet's PROOF table.

### Progress cadence

The UI end (`sftp-ui/transfer.rs::run_transfer`) only stores the latest fraction, and
`transfer::send_progress` uses `try_send` on a bounded channel and deliberately drops a sample
when the consumer is behind. So the UI has no *minimum* cadence requirement — but it does have a
shape the queue row depends on: **monotonically increasing fractions, roughly one per 255 KiB,
and exactly one terminal `Progress(1.0)`**.

Reading through the library would break the "roughly one per 255 KiB" half by accident.
russh-sftp asks the server for `max_packet_len - READ_OVERHEAD_LENGTH` = 262 131 B per request
while `CHUNK_LEN` is 261 120 B, so a `read` into a `CHUNK_LEN` buffer returns 261 120, then the
1 011-byte remainder, then 261 120 ... — a per-read callback would emit ~2x the events, half of
them 0.4 % apart.

So `copy_sequential` emits **when the running total has advanced by at least `CHUNK_LEN` since
the last emit, plus once at the end if anything is left unreported**. Cadence is then a property
of the byte count rather than of the transport's read granularity, which is what the UI wants and
what survives the next library change. Uploads are unaffected: a local `read` fills the whole
`CHUNK_LEN` buffer, so every iteration crosses the threshold and the event sequence is
byte-identical to today's (the existing
`sequential_copy_moves_everything_and_counts_bytes` assertion still passes unmodified).

A 5 MiB download therefore reports 21 samples (20 threshold crossings + the tail), and
`sftp_download` appends its `Progress(1.0)`.

### Resume

**There is no resume feature in OneTerm today**, and this change does not add one.
`grep -rni "resume" crates/sftp-ui/src crates/ssh/src` returns nothing. A download writes into a
fresh `.part` sibling; a cancelled or failed one deletes that sibling and the next attempt starts
at byte 0. That is the shipped contract before and after.

What the change does is make resume *cheap to add later*: a sequential reader resumes with one
`file.seek(SeekFrom::Start(offset))` and then the same loop, where the striped path would have
needed the offset folded into every stripe index. `poll_seek` resets the read queue once, which
costs nothing when it happens once per transfer rather than once per chunk.

### Shrinking file, EOF, and the announced length

Three cases, all handled by the same two lines:

| Remote file | `copy_sequential(reader.take(total))` |
|---|---|
| exactly `total` | `take` hits its limit; the loop ends |
| size-less (`total == 0`) | `announced` is `u64::MAX`; the loop ends at EOF |
| **shrank** below `total` | the library returns `Ok(0)` at EOF; the loop ends early, `on_bytes` has reported the real byte count, and `sftp_download` still sends `Progress(1.0)` — identical to `copy_striped`'s `chunk_count.min(index + 1)` clamp |
| **grew** past `total` | `take` stops at `total`; the extra bytes are never requested — identical to the old `remaining = total - index * CHUNK_LEN` arithmetic |

A zero-length file has `total == 0`, so `announced` is `u64::MAX` and `copy_sequential` reads to
EOF, which is immediate. `sftp_download`'s `on_bytes` already maps `total == 0` to fraction
`1.0`, and it appends `Progress(1.0)` unconditionally afterwards, so the UI never depends on a
pipeline sample for an empty file (`copy_sequential` correctly emits none).

### The settings

`session::sftp_config()`:

| Field | Before | After | Why |
|---|---|---|---|
| `max_concurrent_reads` | `1` (US-0095 Change G) | **`16`** | 3.0's default, and now the only read pipeline. 16 x ~256 KiB ~ 4.2 MB in flight |
| `max_read_packet_len` | — | **not set** | russh-sftp 3.0 has no such field. The read request length is `max_packet_len - 13` = 262 131 B, derived in `ReadState::request`, further clamped by the server's `limits@openssh.com` `read_len`. Nothing to choose |
| `max_write_packet_len` | `262_144` | `262_144` | unchanged (US-0095 Change F) |
| `max_concurrent_writes` | `8` | `8` | unchanged (US-0095 Change F) |

`CHUNK_LEN` (255 KiB) stays: it is the *write* chunk and the progress-cadence unit, and the write
budget test asserts one packet per chunk.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| A later russh-sftp bump changes `max_concurrent_reads`' default or re-introduces a read cap, silently losing the pipeline | Medium | Throughput only | `pipeline_budget_tests` asserts the in-flight read budget is >= `16 x` one packet and that a 5 MiB download costs one READ per packet-length chunk; the D2 regression case is now `max_concurrent_reads: 1` |
| Cancellation leaves queued READs leaking session state | Low | Memory / stuck session | Read the library: `Drop for Request` de-registers, late replies are ignored, `Drop for File` is `close_nowait`. Asserted by a cancel test that then completes a second transfer on the same session. `BUG-0076` found the one leak this missed (the client-side handle count); `handle_limit_tests` guards it |
| Up to 4.2 MB crosses the wire after a cancel | Certain | Bandwidth after cancel | Accepted and measured; it is the same budget that buys the throughput. Documented in `docs/sftp-browser-design.md` |
| Progress cadence changes shape and the queue row jitters | Low | Cosmetic | Cadence is thresholded on bytes, not reads; asserted by an event-count test |
| Losing the multi-handle path hurts a server that serves one handle slowly | Low | Throughput on exotic servers | Not observed; `limits@openssh.com` clamps per-request size, not per-handle rate. Reversible — the striped code is in git history |

## Detail Design

- [x] Detail design: not needed
- Reason: normal lane; the whole change is two functions in one file plus one config field, and
  the mechanics that would fill an LLD (the library's read/seek/drop behaviour, the cadence
  arithmetic, the three EOF cases) are stated above with their source line references. The
  measurement tables live in `US-0096`'s PROOF.
