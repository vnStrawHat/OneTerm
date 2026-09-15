//! The SFTP transfer budget `sftp_config()` pins, and the measurements behind it.
//!
//! Two intakes meet here:
//!
//! - **`IN-0036` / `US-0095` Change F (writes).** russh-sftp 3.0 added
//!   `max_write_packet_len` (32 KiB) as a third cap on every `SSH_FXP_WRITE`,
//!   which with OneTerm's 255 KiB chunks cuts the in-flight write budget 4x.
//!   `sftp_config()` raises it back. **Uploads are not otherwise touched by
//!   `IN-0037`**, and that half of this file is unchanged by it.
//! - **`IN-0037` (reads).** russh-sftp answers a read by putting
//!   `max_concurrent_reads` `SSH_FXP_READ` packets on the wire immediately.
//!   OneTerm used to throw that away by seeking per chunk across striped
//!   handles; the striping is gone and the library's read-ahead is now the only
//!   download pipeline, so `max_concurrent_reads` is back at 3.0's 16 and
//!   **depth 1 is the regression** these tests guard.
//!
//! These tests run a counting SFTP server over an in-process duplex pipe and
//! assert the budget, so a later russh-sftp bump that changes either default
//! fails here instead of silently costing throughput or bandwidth.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write as _};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use russh_sftp::protocol::{
    Attrs, Data, FileAttributes, Handle, OpenFlags, Status, StatusCode, Version,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use oneterm_core::AppError;

use super::{CHUNK_LEN, copy_sequential};
use crate::session::sftp_config;

/// 2.3.0's in-flight write budget: `max_concurrent_writes` 8 x a 255 KiB chunk
/// in one packet. Change F exists to keep at least this much on the wire.
const BASELINE_IN_FLIGHT_WRITE_BYTES: usize = 8 * 261_120;

/// Bytes russh-sftp asks for per `SSH_FXP_READ`: `max_packet_len` minus its
/// 13-byte read overhead (`fs/file.rs READ_OVERHEAD_LENGTH`). 3.0 has no
/// `max_read_packet_len` to tune separately.
const READ_PACKET_LEN: usize = 262_144 - 13;

/// What the retired striped download kept on the wire: `read_handles_for`'s four
/// handles, one 255 KiB request each. The library's read-ahead must beat it, or
/// `IN-0037` traded throughput for a smaller diff.
const RETIRED_STRIPED_IN_FLIGHT_BYTES: usize = 4 * CHUNK_LEN;

// ---------------------------------------------------------------------------
// A counting SFTP server over one real file
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Counters {
    reads: AtomicUsize,
    bytes_read: AtomicUsize,
    writes: AtomicUsize,
    bytes_written: AtomicUsize,
    max_write_packet: AtomicUsize,
}

struct CountingServer {
    path: std::path::PathBuf,
    file: Option<std::fs::File>,
    counters: Arc<Counters>,
}

fn ok_status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

impl russh_sftp::server::Handler for CountingServer {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn open(
        &mut self,
        id: u32,
        _filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        self.file = Some(
            std::fs::OpenOptions::new()
                .read(true)
                .write(pflags.contains(OpenFlags::WRITE))
                .create(pflags.contains(OpenFlags::CREATE))
                .truncate(pflags.contains(OpenFlags::TRUNCATE))
                .open(&self.path)
                .map_err(|_| StatusCode::Failure)?,
        );
        Ok(Handle {
            id,
            handle: "h".to_owned(),
        })
    }

    async fn close(&mut self, id: u32, _handle: String) -> Result<Status, Self::Error> {
        self.file = None;
        Ok(ok_status(id))
    }

    async fn read(
        &mut self,
        id: u32,
        _handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let file = self.file.as_mut().ok_or(StatusCode::BadMessage)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| StatusCode::Failure)?;
        let mut data = vec![0u8; len as usize];
        let mut filled = 0;
        while filled < data.len() {
            let read = file
                .read(&mut data[filled..])
                .map_err(|_| StatusCode::Failure)?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        if filled == 0 {
            return Err(StatusCode::Eof);
        }
        data.truncate(filled);
        self.counters.reads.fetch_add(1, Ordering::SeqCst);
        self.counters
            .bytes_read
            .fetch_add(data.len(), Ordering::SeqCst);
        Ok(Data { id, data })
    }

    async fn write(
        &mut self,
        id: u32,
        _handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let file = self.file.as_mut().ok_or(StatusCode::BadMessage)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| StatusCode::Failure)?;
        file.write_all(&data).map_err(|_| StatusCode::Failure)?;
        self.counters.writes.fetch_add(1, Ordering::SeqCst);
        self.counters
            .bytes_written
            .fetch_add(data.len(), Ordering::SeqCst);
        self.counters
            .max_write_packet
            .fetch_max(data.len(), Ordering::SeqCst);
        Ok(ok_status(id))
    }

    async fn fstat(&mut self, id: u32, _handle: String) -> Result<Attrs, Self::Error> {
        let meta = std::fs::metadata(&self.path).map_err(|_| StatusCode::Failure)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&meta),
        })
    }
}

struct TempPath(std::path::PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn temp_path(tag: &str) -> TempPath {
    use std::sync::atomic::AtomicU64;
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    TempPath(std::env::temp_dir().join(format!(
        "oneterm-sftp-budget-{}-{tag}-{sequence}",
        std::process::id()
    )))
}

/// An SFTP client over a counting server serving `path`, using `config`.
async fn counting_session(
    path: &std::path::Path,
    config: russh_sftp::client::Config,
) -> (russh_sftp::client::SftpSession, Arc<Counters>) {
    let counters = Arc::new(Counters::default());
    let (client_side, server_side) = tokio::io::duplex(8 * 1024 * 1024);
    russh_sftp::server::run(
        server_side,
        CountingServer {
            path: path.to_path_buf(),
            file: None,
            counters: counters.clone(),
        },
    )
    .await;
    let session = russh_sftp::client::SftpSession::new_with_config(client_side, config)
        .await
        .expect("sftp handshake");
    (session, counters)
}

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 253) as u8).collect()
}

// ---------------------------------------------------------------------------
// Change F — the write budget
// ---------------------------------------------------------------------------

/// A 5 MiB upload through `sftp_config()` must keep at least as many bytes in
/// flight as russh-sftp 2.3.0 did.
///
/// With 3.0's defaults the same upload is 161 packets of 32 742 bytes — 523 872
/// bytes in flight against 2.3.0's 2 088 960, a 4x cut that costs ~4x the wall
/// time on any link with real latency.
#[tokio::test]
async fn an_upload_keeps_the_2_3_0_write_budget_in_flight() {
    const SIZE: usize = 5 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("write-budget");
    std::fs::write(&remote.0, b"").expect("create remote file");

    let (sftp, counters) = counting_session(&remote.0, sftp_config()).await;
    let mut file = sftp
        .open_with_flags(
            "payload",
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
        )
        .await
        .expect("open for write");
    for chunk in data.chunks(CHUNK_LEN) {
        file.write_all(chunk).await.expect("upload chunk");
    }
    file.shutdown().await.expect("flush and close");

    assert_eq!(
        std::fs::read(&remote.0).expect("read back"),
        data,
        "the upload must be byte-identical"
    );

    let packets = counters.writes.load(Ordering::SeqCst);
    let largest = counters.max_write_packet.load(Ordering::SeqCst);
    let in_flight = largest * sftp_config().max_concurrent_writes;
    eprintln!(
        "US-0095 F: 5 MiB upload in {packets} WRITE packets (largest {largest} B); \
         in-flight budget {in_flight} B vs russh-sftp 2.3.0's {BASELINE_IN_FLIGHT_WRITE_BYTES} B"
    );

    assert!(
        in_flight >= BASELINE_IN_FLIGHT_WRITE_BYTES,
        "the in-flight write budget regressed: {in_flight} B < {BASELINE_IN_FLIGHT_WRITE_BYTES} B \
         ({packets} packets, largest {largest} B). russh-sftp's max_write_packet_len default \
         probably changed again — see session::sftp_config."
    );
    // One packet per chunk, as 2.3.0 sent. 5 MiB / 255 KiB = 21 chunks.
    assert_eq!(
        packets,
        SIZE.div_ceil(CHUNK_LEN),
        "every chunk must still go out as one packet"
    );
}

/// `IN-0037` changed `copy_sequential`'s progress cadence from once-per-read to
/// once per `CHUNK_LEN` of bytes, and **uploads share that function**. The write
/// budget test above proves the bytes and the packets are untouched; it says
/// nothing about the samples. This pins the sample sequence of a real upload - a
/// real local file into a real `SftpSession`, the exact composition
/// `upload::upload_file_contents` uses - so "uploads are unchanged" covers
/// progress too, measured rather than inferred.
///
/// The sequence asserted here is the one the pre-`IN-0037` per-read cadence
/// produced, because `tokio::fs::File` fills the whole `CHUNK_LEN` buffer from a
/// regular file on every read but the last. A source that short-read would
/// coalesce two former samples into one; that is harmless - `send_progress`
/// drops samples on a full channel by design and `sftp-ui::run_transfer` keeps
/// only the latest fraction - but it is why this test uses a real file rather
/// than a `Cursor`.
#[tokio::test]
async fn an_upload_reports_the_same_progress_samples_it_did_before_in0037() {
    const SIZE: usize = 5 * 1024 * 1024 + 99;
    let source = temp_path("upload-cadence-src");
    std::fs::write(&source.0, payload(SIZE)).expect("write local source file");
    let remote = temp_path("upload-cadence-dst");
    std::fs::write(&remote.0, b"").expect("create remote file");

    let (sftp, _counters) = counting_session(&remote.0, sftp_config()).await;
    let mut local_file = tokio::fs::File::open(&source.0)
        .await
        .expect("open local source");
    let mut remote_file = sftp
        .open_with_flags(
            "payload",
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
        )
        .await
        .expect("open for write");

    let mut progress = Vec::new();
    copy_sequential(
        &mut local_file,
        &mut remote_file,
        &CancellationToken::new(),
        &mut |done| progress.push(done),
    )
    .await
    .expect("upload");
    remote_file.shutdown().await.expect("flush and close");

    assert_eq!(
        std::fs::read(&remote.0).expect("read back"),
        payload(SIZE),
        "the upload must be byte-identical"
    );

    // One sample at every CHUNK_LEN boundary, then the 99-byte tail at EOF.
    let expected: Vec<u64> = (1..=SIZE / CHUNK_LEN)
        .map(|n| (n * CHUNK_LEN) as u64)
        .chain(std::iter::once(SIZE as u64))
        .collect();
    assert_eq!(
        progress,
        expected,
        "an upload's progress samples changed: {} samples, expected {}",
        progress.len(),
        expected.len()
    );
}

// ---------------------------------------------------------------------------
// IN-0037 — the read budget, now owned entirely by the library
// ---------------------------------------------------------------------------

/// Exactly what `transfer::download::download_file_contents` composes: one
/// handle, `take(announced)`, `copy_sequential`. Anything asserted below is
/// therefore asserted about the shipped download path.
async fn download(
    sftp: &russh_sftp::client::SftpSession,
    announced: u64,
    cancel: &CancellationToken,
    on_bytes: &mut impl FnMut(u64),
) -> (Vec<u8>, oneterm_core::Result<()>) {
    let reader = sftp.open("payload").await.expect("open for read");
    let mut reader = reader.take(announced);
    let mut sink = Vec::new();
    let result = copy_sequential(&mut reader, &mut sink, cancel, on_bytes).await;
    (sink, result)
}

/// Read the way the retired striped download did — seek, then read one chunk —
/// so the access pattern that fought the read-ahead stays measurable.
async fn read_seek_per_chunk(
    file: &mut russh_sftp::client::fs::File,
    total: usize,
) -> (Vec<u8>, std::time::Duration) {
    let started = std::time::Instant::now();
    let mut got = Vec::with_capacity(total);
    let mut buffer = vec![0u8; CHUNK_LEN];
    let mut offset = 0usize;
    while offset < total {
        let len = CHUNK_LEN.min(total - offset);
        file.seek(SeekFrom::Start(offset as u64))
            .await
            .expect("seek to the stripe");
        file.read_exact(&mut buffer[..len])
            .await
            .expect("read one chunk");
        got.extend_from_slice(&buffer[..len]);
        offset += len;
    }
    (got, started.elapsed())
}

/// The `IN-0037` contract: a download costs exactly one server READ per read
/// packet, serves the file once, and keeps `max_concurrent_reads` packets on the
/// wire. Depth 1 — what `US-0095` shipped while the striping supplied the
/// concurrency — is measured alongside as the regression case.
#[tokio::test]
async fn a_download_costs_one_read_per_packet_and_keeps_the_read_ahead_in_flight() {
    const SIZE: usize = 5 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("read-budget");
    std::fs::write(&remote.0, &data).expect("write remote file");
    let expected_reads = SIZE.div_ceil(READ_PACKET_LEN);

    // The shipped configuration.
    let (sftp, counters) = counting_session(&remote.0, sftp_config()).await;
    let started = std::time::Instant::now();
    let (got, result) = download(&sftp, SIZE as u64, &CancellationToken::new(), &mut |_| {}).await;
    let chosen_time = started.elapsed();
    result.expect("download");
    assert_eq!(got, data, "the download must be byte-identical");
    let chosen_reads = counters.reads.load(Ordering::SeqCst);
    let chosen_served = counters.bytes_read.load(Ordering::SeqCst);
    drop(sftp);

    // The regression: the library's read-ahead turned off. Same bytes, but only
    // one request on the wire at a time, so throughput collapses with RTT.
    let depth_one = russh_sftp::client::Config {
        max_concurrent_reads: 1,
        ..sftp_config()
    };
    let (sftp, counters) = counting_session(&remote.0, depth_one).await;
    let started = std::time::Instant::now();
    let (got, result) = download(&sftp, SIZE as u64, &CancellationToken::new(), &mut |_| {}).await;
    let depth_one_time = started.elapsed();
    result.expect("download at depth 1");
    assert_eq!(got, data, "depth 1 must still be correct, only slower");
    let depth_one_reads = counters.reads.load(Ordering::SeqCst);
    drop(sftp);

    let in_flight = sftp_config().max_concurrent_reads * READ_PACKET_LEN;
    eprintln!(
        "IN-0037: 5 MiB download, {SIZE} bytes wanted\n\
         \x20 shipped (read-ahead {}): {chosen_reads} READs, {chosen_served} B ({:.2}x), \
         {in_flight} B in flight, {chosen_time:?}\n\
         \x20 regression (read-ahead 1): {depth_one_reads} READs, {} B in flight, \
         {depth_one_time:?}\n\
         \x20 retired striping         : 21 READs, {RETIRED_STRIPED_IN_FLIGHT_BYTES} B in flight",
        sftp_config().max_concurrent_reads,
        chosen_served as f64 / SIZE as f64,
        READ_PACKET_LEN,
    );

    // One server READ per read packet, and not one byte more than the file.
    // Nothing seeks any more, so unlike the striped path there is no per-request
    // slack to allow for: this is exact.
    assert_eq!(
        chosen_reads, expected_reads,
        "a download must cost one READ per read packet, got {chosen_reads} for {expected_reads}"
    );
    assert_eq!(
        chosen_served, SIZE,
        "the server must serve exactly the file, got {chosen_served} B for {SIZE} B"
    );

    // The read-ahead is the only download pipeline OneTerm has left, so it must
    // beat what the striping used to keep on the wire.
    assert!(
        in_flight > RETIRED_STRIPED_IN_FLIGHT_BYTES,
        "the read budget regressed below the striping IN-0037 deleted: {in_flight} B <= \
         {RETIRED_STRIPED_IN_FLIGHT_BYTES} B. russh-sftp's max_concurrent_reads default probably \
         changed — see session::sftp_config."
    );
    assert!(
        sftp_config().max_concurrent_reads >= 16,
        "max_concurrent_reads is {}, not russh-sftp 3.0's 16. Since IN-0037 nothing seeks per \
         chunk, so there is no reason to hold the read-ahead down — see session::sftp_config.",
        sftp_config().max_concurrent_reads
    );
}

/// Why the striping had to go rather than the read-ahead (`US-0095` D2): a seek
/// calls `ReadState::reset`, which drops the queued requests *locally* after the
/// server has already served them. Keep measuring it, so re-introducing a seek
/// into the download loop fails here instead of on someone's bandwidth bill.
#[tokio::test]
async fn seeking_per_chunk_still_throws_the_read_ahead_away() {
    const SIZE: usize = 5 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("seek-amplification");
    std::fs::write(&remote.0, &data).expect("write remote file");

    let (sftp, counters) = counting_session(&remote.0, sftp_config()).await;
    let mut file = sftp.open("payload").await.expect("open for read");
    let (got, elapsed) = read_seek_per_chunk(&mut file, SIZE).await;
    assert_eq!(got, data, "the seek-per-chunk read must still be correct");
    let served = counters.bytes_read.load(Ordering::SeqCst);
    let reads = counters.reads.load(Ordering::SeqCst);

    eprintln!(
        "IN-0037 D2: seek-per-chunk against the shipped read-ahead: {reads} READs, {served} B \
         ({:.2}x the file), {elapsed:?}",
        served as f64 / SIZE as f64
    );
    assert!(
        served > SIZE,
        "expected seek-per-chunk to over-read against a read-ahead of {}, got {served} B for \
         {SIZE} B. If this stopped amplifying, the read-ahead is off — see session::sftp_config.",
        sftp_config().max_concurrent_reads
    );
}

/// A cancel is observed within one chunk, and the teardown is clean: the same
/// session serves a full download straight afterwards, so the reads left in
/// flight neither leaked nor blocked.
#[tokio::test]
async fn a_cancelled_download_stops_within_one_chunk_and_the_session_survives() {
    const SIZE: usize = 5 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("cancel");
    std::fs::write(&remote.0, &data).expect("write remote file");

    let (sftp, counters) = counting_session(&remote.0, sftp_config()).await;
    let cancel = CancellationToken::new();
    let mut at_cancel = 0u64;
    // Cancel half way, so the read-ahead is warm and the discarded bytes below
    // are the budget's real cost rather than the first probe request's.
    let (partial, result) = download(&sftp, SIZE as u64, &cancel, &mut |done| {
        if at_cancel == 0 && done >= (SIZE / 2) as u64 {
            at_cancel = done;
            cancel.cancel();
        }
    })
    .await;

    assert!(
        matches!(result, Err(AppError::Cancelled)),
        "a cancelled download must report Cancelled, not an error"
    );
    assert!(
        at_cancel > 0,
        "the cancel must have been driven by progress"
    );
    assert!(
        (partial.len() as u64) < at_cancel + CHUNK_LEN as u64,
        "the copy ran {} B past the cancel, more than one chunk",
        partial.len() as u64 - at_cancel
    );
    assert_eq!(
        partial,
        data[..partial.len()],
        "what was written before the cancel must still be the file's prefix"
    );

    // Up to `max_concurrent_reads` packets were already on the wire and are
    // discarded by the dropped reader. That bandwidth is the price of the
    // in-flight budget; it must never reach the caller's bytes.
    let served_after_cancel = counters.bytes_read.load(Ordering::SeqCst) - partial.len();
    eprintln!(
        "IN-0037 cancel: stopped at {at_cancel} B, wrote {} B, {served_after_cancel} B served and \
         discarded (budget {} B)",
        partial.len(),
        sftp_config().max_concurrent_reads * READ_PACKET_LEN
    );

    // The real assertion: nothing leaked or wedged the session.
    let (whole, result) =
        download(&sftp, SIZE as u64, &CancellationToken::new(), &mut |_| {}).await;
    result.expect("a second download on the same session after a cancel");
    assert_eq!(whole, data, "the session must still serve a whole file");
}

/// The shrinking-file clamp. A file shorter than the size `stat` announced —
/// which is what a file that shrank mid-transfer looks like on the wire — ends
/// at EOF without an error, and the last progress sample is the real size.
#[tokio::test]
async fn a_file_shorter_than_announced_ends_at_eof_with_the_real_size() {
    let data = payload(CHUNK_LEN + 10);
    let remote = temp_path("shrunk");
    std::fs::write(&remote.0, &data).expect("write remote file");

    let (sftp, _counters) = counting_session(&remote.0, sftp_config()).await;
    let mut progress = Vec::new();
    // `stat` said three chunks; only one and a bit is there.
    let (got, result) = download(
        &sftp,
        (CHUNK_LEN * 3) as u64,
        &CancellationToken::new(),
        &mut |done| progress.push(done),
    )
    .await;

    result.expect("a short file is not an error");
    assert_eq!(got, data);
    assert_eq!(
        progress.last().copied(),
        Some(data.len() as u64),
        "the final size reported must be the real one"
    );
}

/// A file *longer* than announced is cut at the announced size: `take` replaces
/// the retired striped path's `total - index * CHUNK_LEN` arithmetic.
#[tokio::test]
async fn a_file_longer_than_announced_is_cut_at_the_announced_size() {
    let data = payload(CHUNK_LEN * 2);
    let remote = temp_path("grown");
    std::fs::write(&remote.0, &data).expect("write remote file");

    let (sftp, _counters) = counting_session(&remote.0, sftp_config()).await;
    let announced = CHUNK_LEN as u64 + 7;
    let (got, result) = download(&sftp, announced, &CancellationToken::new(), &mut |_| {}).await;

    result.expect("download");
    assert_eq!(got, data[..CHUNK_LEN + 7], "must not over-read");
}

/// The two sizes with no interior: nothing at all, and exactly one read packet.
#[tokio::test]
async fn a_zero_length_file_and_a_one_packet_file_download_exactly() {
    let empty = temp_path("empty");
    std::fs::write(&empty.0, b"").expect("write empty remote file");
    let (sftp, counters) = counting_session(&empty.0, sftp_config()).await;
    let mut progress = Vec::new();
    // A size-less or empty file is announced as `u64::MAX` by
    // `download_file_contents` and read to EOF.
    let (got, result) = download(&sftp, u64::MAX, &CancellationToken::new(), &mut |done| {
        progress.push(done)
    })
    .await;
    result.expect("empty download");
    assert!(got.is_empty());
    assert!(progress.is_empty(), "no bytes, no progress samples");
    assert_eq!(
        counters.reads.load(Ordering::SeqCst),
        0,
        "an empty file costs no served READ (the one request answers Eof)"
    );
    drop(sftp);

    let data = payload(READ_PACKET_LEN);
    let one_packet = temp_path("one-packet");
    std::fs::write(&one_packet.0, &data).expect("write remote file");
    let (sftp, counters) = counting_session(&one_packet.0, sftp_config()).await;
    let (got, result) = download(
        &sftp,
        READ_PACKET_LEN as u64,
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;
    result.expect("one-packet download");
    assert_eq!(got, data);
    assert_eq!(
        counters.reads.load(Ordering::SeqCst),
        1,
        "a file exactly one read packet long must cost exactly one READ"
    );
}

/// The cadence `crates/sftp-ui` renders: monotonic samples, one per `CHUNK_LEN`
/// of bytes, ending on the real size. Not one per read — russh-sftp's 262 131 B
/// responses do not line up with the 261 120 B chunk, so a per-read callback
/// would emit 41 samples here instead of 21.
#[tokio::test]
async fn a_download_reports_one_progress_sample_per_chunk() {
    const SIZE: usize = 5 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("cadence");
    std::fs::write(&remote.0, &data).expect("write remote file");

    let (sftp, _counters) = counting_session(&remote.0, sftp_config()).await;
    let mut progress = Vec::new();
    let (got, result) = download(&sftp, SIZE as u64, &CancellationToken::new(), &mut |done| {
        progress.push(done)
    })
    .await;

    result.expect("download");
    assert_eq!(got, data);
    assert!(progress.windows(2).all(|w| w[0] < w[1]), "monotonic");
    assert_eq!(progress.last().copied(), Some(SIZE as u64));
    assert_eq!(
        progress.len(),
        SIZE.div_ceil(CHUNK_LEN),
        "one sample per chunk of bytes: {} samples for {SIZE} B",
        progress.len()
    );
}
