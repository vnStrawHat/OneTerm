//! The SFTP transfer budget `sftp_config()` pins, and the measurement behind it.
//!
//! russh-sftp 3.0's defaults are not a drop-in for 2.3.0's on either direction
//! (`IN-0036`, `US-0095` Changes F and G):
//!
//! - it added `max_write_packet_len` (32 KiB) as a third cap on every
//!   `SSH_FXP_WRITE`, which with OneTerm's 255 KiB chunks cuts the in-flight
//!   write budget 4x;
//! - it answers a read by putting `max_concurrent_reads` `SSH_FXP_READ` packets
//!   on the wire immediately, which OneTerm's seek-per-chunk striped download
//!   discards after the server has already served them.
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

use super::CHUNK_LEN;
use crate::session::sftp_config;

/// 2.3.0's in-flight write budget: `max_concurrent_writes` 8 x a 255 KiB chunk
/// in one packet. Change F exists to keep at least this much on the wire.
const BASELINE_IN_FLIGHT_WRITE_BYTES: usize = 8 * 261_120;

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
    let (client_side, server_side) = tokio::io::duplex(1024 * 1024);
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

// ---------------------------------------------------------------------------
// Change G — the read budget, both candidate fixes measured
// ---------------------------------------------------------------------------

/// Read a file the way `transfer::pipeline::read_chunk` does: seek, then read
/// exactly one chunk. Returns the reassembled bytes.
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

/// The measurement behind Change G: both candidate fixes on the same 5 MiB
/// download, against the shipped defaults that motivated the change.
///
/// A: OneTerm's seek-per-chunk striping with `max_concurrent_reads: 1`.
/// B: no striping (one sequential `read_to_end`) with 3.0's 16-deep read-ahead.
/// Control: seek-per-chunk against 3.0's defaults — the 3.6x amplification the
/// verification measured.
///
/// The assertion is on the chosen configuration: `sftp_config()` must make the
/// server serve exactly the file, no more.
#[tokio::test]
async fn a_download_serves_exactly_the_file_and_both_candidates_are_measured() {
    const SIZE: usize = 5 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("read-budget");
    std::fs::write(&remote.0, &data).expect("write remote file");

    // Control: 3.0's defaults under OneTerm's access pattern.
    let (sftp, counters) = counting_session(&remote.0, russh_sftp::client::Config::default()).await;
    let mut file = sftp.open("payload").await.expect("open for read");
    let (got, control_time) = read_seek_per_chunk(&mut file, SIZE).await;
    assert_eq!(got, data, "the control read must still be correct");
    let control_served = counters.bytes_read.load(Ordering::SeqCst);
    let control_reads = counters.reads.load(Ordering::SeqCst);
    drop(file);
    drop(sftp);

    // Candidate A — keep the striping, turn the library's read-ahead off.
    let (sftp, counters) = counting_session(&remote.0, sftp_config()).await;
    let mut file = sftp.open("payload").await.expect("open for read");
    let (got, a_time) = read_seek_per_chunk(&mut file, SIZE).await;
    assert_eq!(got, data, "candidate A must be byte-identical");
    let a_served = counters.bytes_read.load(Ordering::SeqCst);
    let a_reads = counters.reads.load(Ordering::SeqCst);
    drop(file);
    drop(sftp);

    // Candidate B — drop the striping, keep the library's read-ahead.
    let (sftp, counters) = counting_session(&remote.0, russh_sftp::client::Config::default()).await;
    let mut file = sftp.open("payload").await.expect("open for read");
    let started = std::time::Instant::now();
    let mut got = Vec::with_capacity(SIZE);
    file.read_to_end(&mut got).await.expect("sequential read");
    let b_time = started.elapsed();
    assert_eq!(got, data, "candidate B must be byte-identical");
    let b_served = counters.bytes_read.load(Ordering::SeqCst);
    let b_reads = counters.reads.load(Ordering::SeqCst);

    let ratio = |served: usize| served as f64 / SIZE as f64;
    eprintln!(
        "US-0095 G: 5 MiB download, {SIZE} bytes wanted\n\
         \x20 control (striping + read-ahead 16): {control_reads} READs, {control_served} B \
         ({:.2}x), {control_time:?}\n\
         \x20 A (striping + read-ahead 1)       : {a_reads} READs, {a_served} B ({:.2}x), {a_time:?}\n\
         \x20 B (no striping + read-ahead 16)   : {b_reads} READs, {b_served} B ({:.2}x), {b_time:?}",
        ratio(control_served),
        ratio(a_served),
        ratio(b_served),
    );

    // Exactly one server READ per chunk OneTerm asked for: no request is issued
    // that a later seek throws away. That is the whole of Change G.
    assert_eq!(
        a_reads,
        SIZE.div_ceil(CHUNK_LEN),
        "the chosen config must cost one READ per chunk, got {a_reads} for {} chunks. \
         russh-sftp's read-ahead is back on — see session::sftp_config.",
        SIZE.div_ceil(CHUNK_LEN)
    );
    // russh-sftp asks for `max_packet_len - READ_OVERHEAD_LENGTH` = 262 131 B per
    // request while OneTerm consumes CHUNK_LEN = 261 120 B, so the seek at the
    // next chunk drops 1 011 B. That 0.4% is a chunk-size mismatch, not
    // read-ahead, and russh-sftp 2.3.0 read the same way; the two overheads
    // (13 for a read, 25 + handle for a write) cannot both be made exact by one
    // `max_packet_len`. Bound it at one request's worth so a real regression
    // still fails here.
    let slack = 262_131 - CHUNK_LEN;
    assert!(
        a_served <= SIZE + slack * SIZE.div_ceil(CHUNK_LEN),
        "the chosen config over-read beyond the known per-request slack: \
         {a_served} B for {SIZE} B in {a_reads} READs"
    );
    // The control is the defect this change fixes: it must really amplify, or
    // the measurement no longer means anything.
    assert!(
        control_served > SIZE,
        "expected 3.0's defaults to over-read under seek-per-chunk, got {control_served} B"
    );
}
