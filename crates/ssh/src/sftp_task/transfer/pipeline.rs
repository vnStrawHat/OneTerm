//! Chunked, pipelined byte copying shared by every SFTP upload and download.
//!
//! SFTP is request/response per chunk, so throughput is bounded by
//! `chunk_size × requests_in_flight / RTT`. Two things keep the pipe full:
//!
//! - **Large chunks.** Every request moves [`CHUNK_LEN`] bytes — sized so one
//!   chunk fits a single 256 KiB SFTP packet even on OpenSSH, whose
//!   `limits@openssh.com` extension caps `read`/`write` payloads at 261 120 bytes.
//! - **Concurrent requests.** Writes are pipelined by the `russh-sftp` `File`
//!   itself (up to `Config::max_concurrent_writes` unacknowledged writes, 8 by
//!   default), so [`copy_sequential`] only needs to feed it big chunks. Reads are
//!   one-request-per-`poll_read`, so [`copy_striped`] opens several handles onto
//!   the same remote file and keeps [`READ_PIPELINE_DEPTH`] reads outstanding,
//!   re-ordering the chunks before they reach the local file.
//!
//! Both helpers observe the [`CancellationToken`] between chunks and report
//! progress as a running byte count through a caller-supplied closure; the
//! caller decides how those bytes map onto the transfer's progress bar and
//! emits the [`TransferEvent`](oneterm_core::TransferEvent)s.

use std::collections::BTreeMap;
use std::io::SeekFrom;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, Result};

/// Bytes moved per SFTP request: 255 KiB, the largest payload that fits one
/// 256 KiB packet under OpenSSH's `limits@openssh.com` read/write caps.
pub(super) const CHUNK_LEN: usize = 255 * 1024;

/// Maximum number of remote read requests kept in flight for one download.
pub(super) const READ_PIPELINE_DEPTH: usize = 4;

/// Chunks that may be requested ahead of the oldest unwritten one. Bounds the
/// re-order buffer when one stripe stalls while the others keep completing.
const REORDER_WINDOW: usize = READ_PIPELINE_DEPTH * 2;

/// How many independent handles a download of `total` bytes should open.
///
/// Every extra handle costs one `open` round trip up front, so small files stay
/// on a single handle and the depth ramps up with the number of chunks.
pub(super) fn read_handles_for(total: u64) -> usize {
    let chunks = total.div_ceil(CHUNK_LEN as u64);
    usize::try_from(chunks / 4)
        .unwrap_or(READ_PIPELINE_DEPTH)
        .clamp(1, READ_PIPELINE_DEPTH)
}

/// Copy `reader` to `writer` in [`CHUNK_LEN`] pieces until EOF.
///
/// `on_bytes` receives the running total of bytes written after every chunk.
/// Cancellation is checked before each read and yields `AppError::Cancelled`.
pub(super) async fn copy_sequential<R, W>(
    reader: &mut R,
    writer: &mut W,
    cancel: &CancellationToken,
    on_bytes: &mut impl FnMut(u64),
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0u8; CHUNK_LEN];
    let mut copied: u64 = 0;
    loop {
        let read = tokio::select! {
            read = reader.read(&mut buffer) => read.map_err(|e| AppError::msg(format!("read: {e}")))?,
            _ = cancel.cancelled() => return Err(AppError::Cancelled),
        };
        if read == 0 {
            return Ok(());
        }
        writer
            .write_all(&buffer[..read])
            .await
            .map_err(|e| AppError::msg(format!("write: {e}")))?;
        copied += read as u64;
        on_bytes(copied);
    }
}

/// Copy the first `total` bytes of a remote file to `writer` using several
/// independent `readers` (each an open handle onto the same file).
///
/// Chunk `i` covers bytes `[i × CHUNK_LEN, (i + 1) × CHUNK_LEN)`. Every reader
/// always has one request outstanding; completed chunks are buffered until all
/// earlier chunks have been written, so `writer` sees the bytes strictly in
/// order. A reader may run at most [`REORDER_WINDOW`] chunks ahead of the
/// oldest unwritten chunk.
///
/// A file that turned out shorter than `total` ends early without error; extra
/// bytes beyond `total` are not read. With `total == 0` nothing is copied — the
/// caller falls back to [`copy_sequential`] for size-less files.
pub(super) async fn copy_striped<R, W>(
    readers: Vec<R>,
    total: u64,
    writer: &mut W,
    cancel: &CancellationToken,
    on_bytes: &mut impl FnMut(u64),
) -> Result<()>
where
    R: AsyncRead + AsyncSeek + Unpin + Send + 'static,
    W: AsyncWrite + Unpin,
{
    if readers.is_empty() {
        return Err(AppError::msg("striped copy needs at least one reader"));
    }
    let mut chunk_count = total.div_ceil(CHUNK_LEN as u64);
    let mut idle = readers;
    let mut in_flight: JoinSet<(u64, R, Result<Vec<u8>>)> = JoinSet::new();
    let mut buffered: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    let mut next_request: u64 = 0;
    let mut next_write: u64 = 0;
    let mut copied: u64 = 0;

    while next_write < chunk_count {
        // Keep every reader busy, as long as the re-order window allows.
        while let Some(mut reader) = idle.pop() {
            let window_open = next_request < next_write + REORDER_WINDOW as u64;
            if next_request >= chunk_count || !window_open {
                idle.push(reader);
                break;
            }
            let index = next_request;
            next_request += 1;
            let remaining = total - index * CHUNK_LEN as u64;
            let len = usize::try_from(remaining.min(CHUNK_LEN as u64)).unwrap_or(CHUNK_LEN);
            in_flight.spawn(async move {
                let result = read_chunk(&mut reader, index * CHUNK_LEN as u64, len).await;
                (index, reader, result)
            });
        }

        let joined = tokio::select! {
            joined = in_flight.join_next() => joined,
            _ = cancel.cancelled() => {
                in_flight.abort_all();
                return Err(AppError::Cancelled);
            }
        };
        let Some(joined) = joined else {
            // Every issued chunk has been written yet the window is closed:
            // impossible with at least one reader, but never spin.
            return Err(AppError::msg("striped copy stalled without readers"));
        };
        let (index, reader, result) =
            joined.map_err(|e| AppError::msg(format!("read task failed: {e}")))?;
        idle.push(reader);
        let data = match result {
            Ok(data) => data,
            Err(error) => {
                in_flight.abort_all();
                return Err(error);
            }
        };
        let expected = (total - index * CHUNK_LEN as u64).min(CHUNK_LEN as u64);
        if (data.len() as u64) < expected {
            // The remote file shrank while we were reading: nothing after this
            // chunk can exist, so the plan ends here (earlier chunks still land).
            chunk_count = chunk_count.min(index + 1);
        }
        buffered.insert(index, data);

        while let Some(data) = buffered.remove(&next_write) {
            writer
                .write_all(&data)
                .await
                .map_err(|e| AppError::msg(format!("write: {e}")))?;
            copied += data.len() as u64;
            next_write += 1;
            on_bytes(copied);
        }
    }
    // Dropping the set aborts reads issued beyond a shrunken end.
    in_flight.abort_all();
    Ok(())
}

/// Read up to `len` bytes at `offset`, stopping early only at EOF.
async fn read_chunk<R>(reader: &mut R, offset: u64, len: usize) -> Result<Vec<u8>>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    reader
        .seek(SeekFrom::Start(offset))
        .await
        .map_err(|e| AppError::msg(format!("seek remote: {e}")))?;
    let mut buffer = vec![0u8; len];
    let mut filled = 0;
    while filled < len {
        let read = reader
            .read(&mut buffer[filled..])
            .await
            .map_err(|e| AppError::msg(format!("read remote: {e}")))?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    buffer.truncate(filled);
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    use super::*;

    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// A reader that answers only every `stride`-th poll, so stripes complete
    /// out of order and the re-order buffer is exercised.
    struct Staggered {
        inner: Cursor<Vec<u8>>,
        stride: usize,
        polls: usize,
        requests: Arc<AtomicUsize>,
    }

    impl AsyncRead for Staggered {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            self.polls += 1;
            if self.polls % self.stride != 0 {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            self.requests.fetch_add(1, Ordering::Relaxed);
            Pin::new(&mut self.inner).poll_read(cx, buf)
        }
    }

    impl AsyncSeek for Staggered {
        fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> std::io::Result<()> {
            Pin::new(&mut self.inner).start_seek(position)
        }

        fn poll_complete(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<std::io::Result<u64>> {
            Pin::new(&mut self.inner).poll_complete(cx)
        }
    }

    #[tokio::test]
    async fn striped_copy_reassembles_chunks_in_order() {
        let data = payload(CHUNK_LEN * 5 + 1234);
        let requests = Arc::new(AtomicUsize::new(0));
        let readers: Vec<Staggered> = (0..READ_PIPELINE_DEPTH)
            .map(|i| Staggered {
                inner: Cursor::new(data.clone()),
                stride: i + 1,
                polls: 0,
                requests: Arc::clone(&requests),
            })
            .collect();
        let mut sink = Vec::new();
        let mut progress = Vec::new();

        copy_striped(
            readers,
            data.len() as u64,
            &mut sink,
            &CancellationToken::new(),
            &mut |bytes| progress.push(bytes),
        )
        .await
        .unwrap();

        assert_eq!(sink, data);
        assert_eq!(progress.last().copied(), Some(data.len() as u64));
        assert!(progress.windows(2).all(|w| w[0] < w[1]));
        // One request per chunk: no chunk was fetched twice or split.
        assert_eq!(requests.load(Ordering::Relaxed), 6);
    }

    #[tokio::test]
    async fn striped_copy_stops_when_the_file_is_shorter_than_announced() {
        let data = payload(CHUNK_LEN + 10);
        let readers = vec![Cursor::new(data.clone()), Cursor::new(data.clone())];
        let mut sink = Vec::new();

        copy_striped(
            readers,
            (CHUNK_LEN * 3) as u64,
            &mut sink,
            &CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();

        assert_eq!(sink, data);
    }

    #[tokio::test]
    async fn striped_copy_reads_only_the_announced_length() {
        let data = payload(CHUNK_LEN * 2);
        let readers = vec![Cursor::new(data.clone())];
        let mut sink = Vec::new();

        copy_striped(
            readers,
            CHUNK_LEN as u64 + 7,
            &mut sink,
            &CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();

        assert_eq!(sink, data[..CHUNK_LEN + 7]);
    }

    #[tokio::test]
    async fn cancelled_copies_report_cancelled() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut sink = Vec::new();

        let striped = copy_striped(
            vec![Cursor::new(payload(10))],
            10,
            &mut sink,
            &cancel,
            &mut |_| {},
        )
        .await;
        assert!(matches!(striped, Err(AppError::Cancelled)));

        let mut source = Cursor::new(payload(10));
        let sequential = copy_sequential(&mut source, &mut sink, &cancel, &mut |_| {}).await;
        assert!(matches!(sequential, Err(AppError::Cancelled)));
        assert!(sink.is_empty());
    }

    #[tokio::test]
    async fn sequential_copy_moves_everything_and_counts_bytes() {
        let data = payload(CHUNK_LEN * 2 + 99);
        let mut source = Cursor::new(data.clone());
        let mut sink = Vec::new();
        let mut progress = Vec::new();

        copy_sequential(
            &mut source,
            &mut sink,
            &CancellationToken::new(),
            &mut |bytes| progress.push(bytes),
        )
        .await
        .unwrap();

        assert_eq!(sink, data);
        assert_eq!(
            progress,
            vec![
                CHUNK_LEN as u64,
                (CHUNK_LEN * 2) as u64,
                (CHUNK_LEN * 2 + 99) as u64
            ]
        );
    }

    #[test]
    fn small_files_use_one_handle_and_large_files_ramp_up() {
        assert_eq!(read_handles_for(0), 1);
        assert_eq!(read_handles_for(CHUNK_LEN as u64 * 3), 1);
        assert_eq!(read_handles_for(CHUNK_LEN as u64 * 8), 2);
        assert_eq!(read_handles_for(CHUNK_LEN as u64 * 16), READ_PIPELINE_DEPTH);
        assert_eq!(read_handles_for(u64::MAX), READ_PIPELINE_DEPTH);
    }
}
