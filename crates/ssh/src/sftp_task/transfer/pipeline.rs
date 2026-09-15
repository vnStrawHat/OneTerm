//! Chunked byte copying shared by every SFTP upload and download.
//!
//! SFTP is request/response per chunk, so throughput is bounded by
//! `bytes in flight / RTT`. Two things keep the pipe full, and **russh-sftp
//! owns both of them** — this module only feeds it:
//!
//! - **Large chunks.** Every write moves [`CHUNK_LEN`] bytes — sized so one
//!   chunk fits a single 256 KiB SFTP packet even on OpenSSH, whose
//!   `limits@openssh.com` extension caps `read`/`write` payloads at 261 120
//!   bytes.
//! - **Concurrent requests.** The `russh-sftp` `File` pipelines by itself:
//!   `Config::max_concurrent_writes` unacknowledged writes, and
//!   `Config::max_concurrent_reads` `SSH_FXP_READ` packets put on the wire as
//!   soon as a read is polled. [`crate::session::sftp_config`] pins 8 and 16.
//!
//! [`copy_sequential`] is therefore the whole engine in both directions.
//!
//! **Do not seek per chunk.** `File::poll_seek` calls `ReadState::reset`, which
//! discards the queued read-ahead *locally* — the server has already served it.
//! OneTerm used to stripe downloads across several handles that way, and with
//! the read-ahead on it made the server send 9.3x the file size (`IN-0036`
//! `US-0095` Change G). `IN-0037` deleted the striping instead: one handle, one
//! sequential pass, the library's 16-deep pipeline. `pipeline_budget_tests`
//! keeps measuring both halves of that.
//!
//! [`copy_sequential`] observes the [`CancellationToken`] between reads and
//! reports progress as a running byte count through a caller-supplied closure;
//! the caller decides how those bytes map onto the transfer's progress bar and
//! emits the [`TransferEvent`](oneterm_core::TransferEvent)s.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, Result};

/// Bytes moved per SFTP write request, and the progress-reporting unit: 255 KiB,
/// the largest payload that fits one 256 KiB packet under OpenSSH's
/// `limits@openssh.com` read/write caps.
pub(super) const CHUNK_LEN: usize = 255 * 1024;

/// Copy `reader` to `writer` in [`CHUNK_LEN`] pieces until EOF.
///
/// `on_bytes` receives the running total of bytes written, once every time that
/// total advances by at least [`CHUNK_LEN`] and once more at EOF if anything is
/// still unreported. Thresholding on **bytes** rather than on reads is what
/// keeps the cadence stable: an upload's local `read` fills the whole buffer
/// every time, but a download's does not — russh-sftp asks the server for
/// `max_packet_len - 13` = 262 131 B while this buffer holds 261 120, so reads
/// alternate between a full buffer and a 1 011-byte remainder. Per-read
/// reporting would emit twice as many samples, half of them 0.4 % apart.
///
/// Cancellation is checked before each read — `biased`, so an already-cancelled
/// token returns without reading a byte — and yields `AppError::Cancelled`. The
/// in-flight read requests are torn down by dropping the reader, which
/// russh-sftp handles without leaking or blocking: `Drop for Request`
/// de-registers the response slot, a reply that arrives for a dropped request is
/// ignored, and `Drop for File` closes the handle without awaiting.
///
/// To stop at a known size rather than at EOF — a download whose `stat` gave a
/// length — wrap the reader in [`tokio::io::AsyncReadExt::take`]. A file that
/// turned out *shorter* then ends early at EOF without error, with `on_bytes`
/// having reported the real byte count.
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
    let mut reported: u64 = 0;
    loop {
        let read = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AppError::Cancelled),
            read = reader.read(&mut buffer) => read.map_err(|e| AppError::msg(format!("read: {e}")))?,
        };
        if read == 0 {
            if copied > reported {
                on_bytes(copied);
            }
            return Ok(());
        }
        writer
            .write_all(&buffer[..read])
            .await
            .map_err(|e| AppError::msg(format!("write: {e}")))?;
        copied += read as u64;
        if copied - reported >= CHUNK_LEN as u64 {
            reported = copied;
            on_bytes(copied);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use super::*;

    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// A reader that hands back at most `serve` bytes per poll, the way
    /// russh-sftp's `ReadState` drains one server response at a time.
    struct Chunked {
        data: Vec<u8>,
        pos: usize,
        serve: usize,
    }

    impl AsyncRead for Chunked {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let take = self
                .serve
                .min(buf.remaining())
                .min(self.data.len() - self.pos);
            let end = self.pos + take;
            let start = self.pos;
            let slice = self.data[start..end].to_vec();
            buf.put_slice(&slice);
            self.pos = end;
            Poll::Ready(Ok(()))
        }
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

    /// The cadence is a property of the byte count, not of how the transport
    /// happens to split its responses. A reader that serves 262 131 B at a time
    /// — russh-sftp's actual read length — must still report once per chunk.
    #[tokio::test]
    async fn progress_cadence_survives_a_reader_that_splits_differently() {
        const TOTAL: usize = 5 * 1024 * 1024;
        let data = payload(TOTAL);
        let mut source = Chunked {
            data: data.clone(),
            pos: 0,
            serve: 262_131,
        };
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
        assert_eq!(progress.last().copied(), Some(TOTAL as u64));
        assert!(progress.windows(2).all(|w| w[0] < w[1]), "monotonic");
        // One sample per CHUNK_LEN of *bytes*, not one per read: 5 MiB / 255 KiB
        // rounded up. Reporting per read would give 41 here.
        assert_eq!(progress.len(), TOTAL.div_ceil(CHUNK_LEN));
    }

    /// `take` is how a download stops at its announced size; a shorter file ends
    /// at EOF instead, with the real byte count reported.
    #[tokio::test]
    async fn take_caps_a_long_file_and_a_short_one_ends_at_eof() {
        let data = payload(CHUNK_LEN * 2);

        let mut sink = Vec::new();
        let mut reader = Cursor::new(data.clone()).take(CHUNK_LEN as u64 + 7);
        copy_sequential(
            &mut reader,
            &mut sink,
            &CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(sink, data[..CHUNK_LEN + 7], "must not over-read");

        let short = payload(CHUNK_LEN + 10);
        let mut sink = Vec::new();
        let mut progress = Vec::new();
        let mut reader = Cursor::new(short.clone()).take(CHUNK_LEN as u64 * 3);
        copy_sequential(
            &mut reader,
            &mut sink,
            &CancellationToken::new(),
            &mut |b| progress.push(b),
        )
        .await
        .unwrap();
        assert_eq!(sink, short, "a shorter file ends without error");
        assert_eq!(
            progress.last().copied(),
            Some(short.len() as u64),
            "the final size reported is the real one"
        );
    }

    #[tokio::test]
    async fn a_zero_length_file_copies_nothing_and_reports_nothing() {
        let mut source = Cursor::new(Vec::new());
        let mut sink = Vec::new();
        let mut progress = Vec::new();

        copy_sequential(
            &mut source,
            &mut sink,
            &CancellationToken::new(),
            &mut |b| progress.push(b),
        )
        .await
        .unwrap();

        assert!(sink.is_empty());
        assert!(progress.is_empty(), "no bytes, no samples");
    }

    #[tokio::test]
    async fn cancelled_copies_report_cancelled() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut sink = Vec::new();
        let mut source = Cursor::new(payload(10));

        let result = copy_sequential(&mut source, &mut sink, &cancel, &mut |_| {}).await;

        assert!(matches!(result, Err(AppError::Cancelled)));
        assert!(sink.is_empty(), "a cancelled copy writes nothing");
    }
}

// The SFTP transfer budget `session::sftp_config()` pins, measured against a
// counting in-process server (see code-style.md on sibling test modules).
#[cfg(test)]
#[path = "pipeline_budget_tests.rs"]
mod pipeline_budget_tests;
