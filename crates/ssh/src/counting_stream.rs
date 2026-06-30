//! `CountingStream` — wrapper đếm bytes read/written qua SFTP channel.
//!
//! Wrap `ChannelStream` (từ `channel.into_stream()`) trước khi truyền vào
//! `SftpSession::new()`. Mỗi byte read/write qua stream → cập nhật
//! `rx_bytes`/`tx_bytes` trong `SharedState` — gộp chung với SSH shell channel.
//!
//! Cả 2 channel (shell + sftp) chia sẻ 1 TCP connection, multiplex bởi russh,
//! nên tổng bytes = SSH shell bytes + SFTP bytes = full network traffic.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::state::SharedState;

/// Wrapper đếm bytes rx/tx qua `AsyncRead + AsyncWrite` stream.
pub(crate) struct CountingStream<S> {
    inner: S,
    state: SharedState,
}

impl<S> CountingStream<S> {
    pub(crate) fn new(inner: S, state: SharedState) -> Self {
        Self { inner, state }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for CountingStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let filled_before = buf.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let filled_after = buf.filled().len();
                let n = filled_after.saturating_sub(filled_before);
                if n > 0 {
                    self.state.lock().unwrap().rx_bytes += n as u64;
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for CountingStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match Pin::new(&mut self.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                if n > 0 {
                    self.state.lock().unwrap().tx_bytes += n as u64;
                }
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}