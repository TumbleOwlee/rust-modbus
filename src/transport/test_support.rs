//! In-memory stand-in for a stream with an operating-system input queue.
//!
//! Bytes the peer has already written count as "received": discarding drains
//! them, as clearing a serial port's input queue does.

use core::pin::Pin;
use core::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};

use crate::transport::DiscardInput;

#[derive(Debug)]
pub(crate) struct DiscardingDuplex {
    inner: DuplexStream,
    failure: Option<std::io::ErrorKind>,
    discards: usize,
}

impl DiscardingDuplex {
    pub(crate) fn new(inner: DuplexStream) -> Self {
        Self {
            inner,
            failure: None,
            discards: 0,
        }
    }

    pub(crate) fn failing(inner: DuplexStream, kind: std::io::ErrorKind) -> Self {
        Self {
            inner,
            failure: Some(kind),
            discards: 0,
        }
    }

    pub(crate) fn discards(&self) -> usize {
        self.discards
    }
}

impl AsyncRead for DiscardingDuplex {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for DiscardingDuplex {
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

impl DiscardInput for DiscardingDuplex {
    fn discard_input(&mut self) -> std::io::Result<()> {
        self.discards += 1;
        if let Some(kind) = self.failure {
            return Err(std::io::Error::from(kind));
        }
        let mut cx = Context::from_waker(core::task::Waker::noop());
        let mut scratch = [0u8; 256];
        loop {
            let mut buf = ReadBuf::new(&mut scratch);
            match Pin::new(&mut self.inner).poll_read(&mut cx, &mut buf) {
                Poll::Ready(Ok(())) if !buf.filled().is_empty() => {}
                Poll::Ready(Ok(())) | Poll::Pending => return Ok(()),
                Poll::Ready(Err(e)) => return Err(e),
            }
        }
    }
}
