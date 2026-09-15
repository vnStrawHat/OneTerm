//! Independent verification of `IN-0037` / `US-0096` (verifier-owned, not part
//! of the shipped branch).
//!
//! These tests are deliberately written from the outside: they sniff the wire
//! rather than trust `sftp_config()` arithmetic, they drive the real
//! `sftp_download` entry point for the staging/cleanup claims, and they cover
//! the failure modes the implementer's own suite does not (transport death
//! mid-copy, destination write failure mid-copy).

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use russh_sftp::protocol::{
    Attrs, Data, FileAttributes, Handle, OpenFlags, Status, StatusCode, Version,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, RemotePath, TransferEvent};

use super::pipeline::{CHUNK_LEN, copy_sequential};
use crate::session::sftp_config;

const READ_PACKET_LEN: usize = 262_144 - 13;

// ---------------------------------------------------------------------------
// A packet-counting wrapper around the client end of the duplex
// ---------------------------------------------------------------------------

#[derive(Default)]
struct WireStats {
    reads_sent: AtomicUsize,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

#[derive(Default)]
struct Framer {
    header: Vec<u8>,
    remaining: usize,
    want_type: bool,
}

impl Framer {
    /// Feed raw bytes, calling `on_type` once per SFTP packet with its type byte.
    fn feed(&mut self, mut bytes: &[u8], on_type: &mut impl FnMut(u8)) {
        while !bytes.is_empty() {
            if self.remaining == 0 && !self.want_type {
                let need = 4 - self.header.len();
                let take = need.min(bytes.len());
                self.header.extend_from_slice(&bytes[..take]);
                bytes = &bytes[take..];
                if self.header.len() == 4 {
                    let len = u32::from_be_bytes([
                        self.header[0],
                        self.header[1],
                        self.header[2],
                        self.header[3],
                    ]) as usize;
                    self.header.clear();
                    self.remaining = len;
                    self.want_type = true;
                }
                continue;
            }
            if self.want_type {
                on_type(bytes[0]);
                bytes = &bytes[1..];
                self.remaining = self.remaining.saturating_sub(1);
                self.want_type = false;
                continue;
            }
            let take = self.remaining.min(bytes.len());
            self.remaining -= take;
            bytes = &bytes[take..];
        }
    }
}

/// Wraps the client side of the duplex and counts `SSH_FXP_READ` packets going
/// out against `SSH_FXP_DATA`/`SSH_FXP_STATUS` packets coming back, so the
/// in-flight read depth is *measured* rather than derived from the config.
struct WireSniffer<S> {
    inner: S,
    out: Framer,
    inp: Framer,
    stats: Arc<WireStats>,
}

const FXP_STATUS: u8 = 101;
const FXP_DATA: u8 = 103;
const FXP_READ: u8 = 5;

impl<S: AsyncRead + Unpin> AsyncRead for WireSniffer<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll {
            let fresh = buf.filled()[before..].to_vec();
            let stats = Arc::clone(&self.stats);
            self.inp.feed(&fresh, &mut |kind| {
                if kind == FXP_DATA || kind == FXP_STATUS {
                    let previous = stats.in_flight.load(Ordering::SeqCst);
                    if previous > 0 {
                        stats.in_flight.store(previous - 1, Ordering::SeqCst);
                    }
                }
            });
        }
        poll
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for WireSniffer<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match Pin::new(&mut self.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                let observed = buf[..n].to_vec();
                let stats = Arc::clone(&self.stats);
                self.out.feed(&observed, &mut |kind| {
                    if kind == FXP_READ {
                        stats.reads_sent.fetch_add(1, Ordering::SeqCst);
                        let now = stats.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        stats.max_in_flight.fetch_max(now, Ordering::SeqCst);
                    }
                });
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// A client stream that reports EOF after `budget` bytes have been read, which
/// is what a dropped SSH channel looks like to the SFTP client.
struct DyingStream<S> {
    inner: S,
    budget: Arc<AtomicUsize>,
}

impl<S: AsyncRead + Unpin> AsyncRead for DyingStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.budget.load(Ordering::SeqCst) == 0 {
            return Poll::Ready(Ok(())); // EOF
        }
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll {
            let got = buf.filled().len() - before;
            let left = self.budget.load(Ordering::SeqCst);
            self.budget
                .store(left.saturating_sub(got), Ordering::SeqCst);
        }
        poll
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for DyingStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// A destination that fails after `ok_bytes` bytes, like a full disk.
struct FailingSink {
    written: usize,
    ok_bytes: usize,
}

impl AsyncWrite for FailingSink {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.written >= self.ok_bytes {
            return Poll::Ready(Err(std::io::Error::other("no space left on device")));
        }
        self.written += buf.len();
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

// ---------------------------------------------------------------------------
// Verifier-owned SFTP server
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ServerCounters {
    reads: AtomicUsize,
    bytes_read: AtomicUsize,
    opens: AtomicUsize,
}

struct VerifyServer {
    path: std::path::PathBuf,
    file: Option<std::fs::File>,
    counters: Arc<ServerCounters>,
    /// Size reported by `lstat`/`fstat`, when it must differ from the real one.
    announce_size: Option<u64>,
    read_delay: Option<Duration>,
}

fn ok_status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

impl VerifyServer {
    fn attrs(&self) -> FileAttributes {
        let meta = std::fs::metadata(&self.path).expect("metadata");
        let mut attrs = FileAttributes::from(&meta);
        if let Some(size) = self.announce_size {
            attrs.size = Some(size);
        }
        attrs
    }
}

impl russh_sftp::server::Handler for VerifyServer {
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

    async fn lstat(&mut self, id: u32, _path: String) -> Result<Attrs, Self::Error> {
        Ok(Attrs {
            id,
            attrs: self.attrs(),
        })
    }

    async fn stat(&mut self, id: u32, _path: String) -> Result<Attrs, Self::Error> {
        Ok(Attrs {
            id,
            attrs: self.attrs(),
        })
    }

    async fn fstat(&mut self, id: u32, _handle: String) -> Result<Attrs, Self::Error> {
        Ok(Attrs {
            id,
            attrs: self.attrs(),
        })
    }

    async fn open(
        &mut self,
        id: u32,
        _filename: String,
        _pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        self.counters.opens.fetch_add(1, Ordering::SeqCst);
        self.file = Some(std::fs::File::open(&self.path).map_err(|_| StatusCode::Failure)?);
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
        if let Some(delay) = self.read_delay {
            tokio::time::sleep(delay).await;
        }
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
        "oneterm-in0037-verify-{}-{tag}-{sequence}",
        std::process::id()
    )))
}

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

struct Harness {
    sftp: russh_sftp::client::SftpSession,
    counters: Arc<ServerCounters>,
    wire: Arc<WireStats>,
    /// Bytes the client may still receive before the transport reports EOF.
    transport_budget: Arc<AtomicUsize>,
}

async fn harness(
    path: &std::path::Path,
    config: russh_sftp::client::Config,
    announce_size: Option<u64>,
    read_delay: Option<Duration>,
) -> Harness {
    let counters = Arc::new(ServerCounters::default());
    let wire = Arc::new(WireStats::default());
    let transport_budget = Arc::new(AtomicUsize::new(usize::MAX));
    let (client_side, server_side) = tokio::io::duplex(8 * 1024 * 1024);
    russh_sftp::server::run(
        server_side,
        VerifyServer {
            path: path.to_path_buf(),
            file: None,
            counters: Arc::clone(&counters),
            announce_size,
            read_delay,
        },
    )
    .await;
    let client = WireSniffer {
        inner: DyingStream {
            inner: client_side,
            budget: Arc::clone(&transport_budget),
        },
        out: Framer::default(),
        inp: Framer::default(),
        stats: Arc::clone(&wire),
    };
    let sftp = russh_sftp::client::SftpSession::new_with_config(client, config)
        .await
        .expect("sftp handshake");
    Harness {
        sftp,
        counters,
        wire,
        transport_budget,
    }
}

/// Exactly what `download::download_file_contents` composes.
async fn download_like_production(
    sftp: &russh_sftp::client::SftpSession,
    announced: u64,
    cancel: &CancellationToken,
    on_bytes: &mut impl FnMut(u64),
) -> (Vec<u8>, oneterm_core::Result<()>) {
    let mut reader = sftp.open("payload").await.expect("open").take(announced);
    let mut sink = Vec::new();
    let result = copy_sequential(&mut reader, &mut sink, cancel, on_bytes).await;
    (sink, result)
}

// ---------------------------------------------------------------------------
// V1 — the in-flight read depth, measured on the wire
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v1_measured_wire_depth_is_the_configured_read_ahead() {
    const SIZE: usize = 5 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("depth");
    std::fs::write(&remote.0, &data).expect("write");

    // A slow server so the client's writer task can get every queued READ onto
    // the wire before the first reply comes back.
    let h = harness(
        &remote.0,
        sftp_config(),
        None,
        Some(Duration::from_millis(3)),
    )
    .await;
    let (got, result) =
        download_like_production(&h.sftp, SIZE as u64, &CancellationToken::new(), &mut |_| {})
            .await;
    result.expect("download");
    assert_eq!(got, data);
    let shipped_depth = h.wire.max_in_flight.load(Ordering::SeqCst);
    let shipped_reads = h.wire.reads_sent.load(Ordering::SeqCst);
    let served = h.counters.bytes_read.load(Ordering::SeqCst);
    let server_reads = h.counters.reads.load(Ordering::SeqCst);
    let opens = h.counters.opens.load(Ordering::SeqCst);
    drop(h);

    let h = harness(
        &remote.0,
        russh_sftp::client::Config {
            max_concurrent_reads: 1,
            ..sftp_config()
        },
        None,
        Some(Duration::from_millis(3)),
    )
    .await;
    let (got, result) =
        download_like_production(&h.sftp, SIZE as u64, &CancellationToken::new(), &mut |_| {})
            .await;
    result.expect("download at depth 1");
    assert_eq!(got, data);
    let depth_one = h.wire.max_in_flight.load(Ordering::SeqCst);

    eprintln!(
        "V1 wire: shipped max in-flight READs {shipped_depth} ({} B), READs sent {shipped_reads}, \
         server READs {server_reads}, served {served} B ({:.2}x), OPENs {opens}; depth-1 arm \
         max in-flight {depth_one}",
        shipped_depth * READ_PACKET_LEN,
        served as f64 / SIZE as f64,
    );

    assert_eq!(opens, 1, "one SSH_FXP_OPEN per download");
    assert_eq!(served, SIZE, "the server must serve exactly the file");
    assert_eq!(
        shipped_depth,
        sftp_config().max_concurrent_reads,
        "the read-ahead measured on the wire must equal max_concurrent_reads"
    );
    assert_eq!(depth_one, 1, "depth 1 must really be one at a time");
}

// ---------------------------------------------------------------------------
// V2 — the 50 MiB table, re-measured
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v2_fifty_mib_table() {
    const SIZE: usize = 50 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("fifty");
    std::fs::write(&remote.0, &data).expect("write");

    let h = harness(&remote.0, sftp_config(), None, None).await;
    let started = std::time::Instant::now();
    let mut samples = 0usize;
    let (got, result) =
        download_like_production(&h.sftp, SIZE as u64, &CancellationToken::new(), &mut |_| {
            samples += 1
        })
        .await;
    let elapsed = started.elapsed();
    result.expect("download");
    assert_eq!(got.len(), SIZE);
    assert_eq!(got, data);
    let served = h.counters.bytes_read.load(Ordering::SeqCst);
    let reads = h.counters.reads.load(Ordering::SeqCst);
    eprintln!(
        "V2 50 MiB: {reads} server READs, {served} B ({:.2}x), {samples} progress samples, \
         {elapsed:?}, OPENs {}",
        served as f64 / SIZE as f64,
        h.counters.opens.load(Ordering::SeqCst)
    );
    assert_eq!(reads, SIZE.div_ceil(READ_PACKET_LEN));
    assert_eq!(served, SIZE);
    assert_eq!(samples, SIZE.div_ceil(CHUNK_LEN));
}

// ---------------------------------------------------------------------------
// V3 — cancel at 50 % of 50 MiB: how much is discarded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_cancel_at_half_of_fifty_mib() {
    const SIZE: usize = 50 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("cancel50");
    std::fs::write(&remote.0, &data).expect("write");

    let h = harness(&remote.0, sftp_config(), None, None).await;
    let cancel = CancellationToken::new();
    let mut at_cancel = 0u64;
    let (partial, result) = download_like_production(&h.sftp, SIZE as u64, &cancel, &mut |done| {
        if at_cancel == 0 && done >= (SIZE / 2) as u64 {
            at_cancel = done;
            cancel.cancel();
        }
    })
    .await;
    assert!(matches!(result, Err(AppError::Cancelled)));
    let served = h.counters.bytes_read.load(Ordering::SeqCst);
    eprintln!(
        "V3 cancel: wrote {} B (cancel fired at {at_cancel} B), server served {served} B, \
         discarded {} B, budget {} B",
        partial.len(),
        served - partial.len(),
        sftp_config().max_concurrent_reads * READ_PACKET_LEN
    );
    assert_eq!(
        partial.len() as u64,
        at_cancel,
        "the writer must stop on the very next loop turn, not one chunk later"
    );
    assert_eq!(partial, data[..partial.len()]);
    assert!(
        served - partial.len() <= sftp_config().max_concurrent_reads * READ_PACKET_LEN,
        "more was discarded than the read-ahead budget can explain"
    );

    // The session must still work.
    let (whole, result) =
        download_like_production(&h.sftp, SIZE as u64, &CancellationToken::new(), &mut |_| {})
            .await;
    result.expect("second download after cancel");
    assert_eq!(whole, data);
}

// ---------------------------------------------------------------------------
// V4 — the transport dies mid-copy: an error, not a hang
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v4_transport_death_mid_copy_is_an_error_not_a_hang() {
    const SIZE: usize = 20 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("dying");
    std::fs::write(&remote.0, &data).expect("write");

    let h = harness(
        &remote.0,
        sftp_config(),
        None,
        Some(Duration::from_millis(1)),
    )
    .await;
    // Let the handshake and a few megabytes through, then report EOF.
    h.transport_budget.store(2 * 1024 * 1024, Ordering::SeqCst);

    let outcome = tokio::time::timeout(
        Duration::from_secs(20),
        download_like_production(&h.sftp, SIZE as u64, &CancellationToken::new(), &mut |_| {}),
    )
    .await;

    let (partial, result) = outcome.expect("the download must not hang when the transport dies");
    let error = result.expect_err("a dead transport must fail the download");
    assert!(
        !matches!(error, AppError::Cancelled),
        "a dead transport is an error, not a cancellation: {error}"
    );
    eprintln!(
        "V4 transport death: {} B written before the failure, error = {error}",
        partial.len()
    );
}

// ---------------------------------------------------------------------------
// V5 — the destination fails mid-copy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v5_destination_write_failure_propagates_without_hanging() {
    const SIZE: usize = 10 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("badsink");
    std::fs::write(&remote.0, &data).expect("write");

    let h = harness(&remote.0, sftp_config(), None, None).await;
    let mut reader = h
        .sftp
        .open("payload")
        .await
        .expect("open")
        .take(SIZE as u64);
    let mut sink = FailingSink {
        written: 0,
        ok_bytes: 3 * CHUNK_LEN,
    };

    let outcome = tokio::time::timeout(
        Duration::from_secs(20),
        copy_sequential(
            &mut reader,
            &mut sink,
            &CancellationToken::new(),
            &mut |_| {},
        ),
    )
    .await
    .expect("a failing destination must not hang the copy");
    let error = outcome.expect_err("the destination error must propagate");
    assert!(
        error.to_string().contains("write"),
        "expected a write error, got {error}"
    );

    // Dropping the reader with the read-ahead still in flight must not wedge
    // the session.
    drop(reader);
    let (got, result) =
        download_like_production(&h.sftp, SIZE as u64, &CancellationToken::new(), &mut |_| {})
            .await;
    result.expect("the session survives a failed destination");
    assert_eq!(got, data);
}

// ---------------------------------------------------------------------------
// V6 / V7 — the real `sftp_download` entry point
// ---------------------------------------------------------------------------

fn temp_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::AtomicU64;
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "oneterm-in0037-dir-{}-{tag}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn leftovers(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

#[tokio::test]
async fn v6_cancelled_download_leaves_no_part_file_and_no_target() {
    const SIZE: usize = 20 * 1024 * 1024;
    let data = payload(SIZE);
    let remote = temp_path("uicancel");
    std::fs::write(&remote.0, &data).expect("write");
    let dir = temp_dir("uicancel");
    let local = dir.join("downloaded.bin");

    let h = harness(&remote.0, sftp_config(), None, None).await;
    let (tx, rx) = async_channel::unbounded::<TransferEvent>();
    let cancel = CancellationToken::new();
    let watcher = {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let mut events = Vec::new();
            while let Ok(event) = rx.recv().await {
                if let TransferEvent::Progress(p) = event {
                    if p >= 0.25 {
                        cancel.cancel();
                    }
                }
                events.push(event);
            }
            events
        })
    };

    let result = tokio::time::timeout(
        Duration::from_secs(30),
        super::sftp_download(&h.sftp, &RemotePath::new("/payload"), &local, &tx, &cancel),
    )
    .await
    .expect("sftp_download must not hang on cancel");
    drop(tx);
    let events = watcher.await.expect("watcher");

    assert!(
        matches!(result, Err(AppError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );
    assert!(
        events.iter().any(|e| matches!(e, TransferEvent::Cancelled)),
        "the UI must see a Cancelled event: {events:?}"
    );
    let left = leftovers(&dir);
    eprintln!("V6 after cancel, directory holds: {left:?}");
    assert!(
        left.is_empty(),
        "a cancelled download must leave neither a .part file nor a target: {left:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn v7_a_file_that_shrank_after_stat_is_reported_as_a_success() {
    let data = payload(CHUNK_LEN + 10);
    let remote = temp_path("shrank");
    std::fs::write(&remote.0, &data).expect("write");
    let dir = temp_dir("shrank");
    let local = dir.join("downloaded.bin");

    // `lstat` announces three chunks; only one and a bit are really there.
    let announced = (CHUNK_LEN * 3) as u64;
    let h = harness(&remote.0, sftp_config(), Some(announced), None).await;
    let (tx, rx) = async_channel::unbounded::<TransferEvent>();
    let collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Ok(event) = rx.recv().await {
            events.push(event);
        }
        events
    });

    let result = super::sftp_download(
        &h.sftp,
        &RemotePath::new("/payload"),
        &local,
        &tx,
        &CancellationToken::new(),
    )
    .await;
    drop(tx);
    let events = collector.await.expect("collector");

    let landed = std::fs::read(&local).expect("downloaded file");
    let last = events
        .iter()
        .rev()
        .find_map(|e| match e {
            TransferEvent::Progress(p) => Some(*p),
            _ => None,
        })
        .unwrap_or(-1.0);
    eprintln!(
        "V7 shrank: announced {announced} B, landed {} B, result ok = {}, last progress {last}",
        landed.len(),
        result.is_ok()
    );
    assert!(result.is_ok(), "a short file is reported as a success");
    assert_eq!(
        landed, data,
        "the destination holds only what the server had"
    );
    assert!(
        landed.len() < announced as usize,
        "this is the silent-truncation case being documented"
    );
    assert!(
        (last - 1.0).abs() < f64::EPSILON,
        "the UI is told the transfer reached 100 %: {last}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn v8_a_file_that_grew_after_stat_is_silently_truncated_to_the_announced_size() {
    let data = payload(CHUNK_LEN * 3);
    let remote = temp_path("grew");
    std::fs::write(&remote.0, &data).expect("write");
    let dir = temp_dir("grew");
    let local = dir.join("downloaded.bin");

    let announced = CHUNK_LEN as u64 + 7;
    let h = harness(&remote.0, sftp_config(), Some(announced), None).await;
    let (tx, rx) = async_channel::unbounded::<TransferEvent>();
    let collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Ok(event) = rx.recv().await {
            events.push(event);
        }
        events
    });

    let result = super::sftp_download(
        &h.sftp,
        &RemotePath::new("/payload"),
        &local,
        &tx,
        &CancellationToken::new(),
    )
    .await;
    drop(tx);
    let _ = collector.await;

    let landed = std::fs::read(&local).expect("downloaded file");
    let served = h.counters.bytes_read.load(Ordering::SeqCst);
    eprintln!(
        "V8 grew: announced {announced} B, landed {} B, server served {served} B, ok = {}",
        landed.len(),
        result.is_ok()
    );
    assert!(result.is_ok());
    assert_eq!(
        landed.len(),
        announced as usize,
        "cut at the announced size"
    );
    assert_eq!(landed, data[..announced as usize]);
    let _ = std::fs::remove_dir_all(&dir);
}
