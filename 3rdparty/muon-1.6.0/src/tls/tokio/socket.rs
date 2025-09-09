use pin_project::pin_project;
use std::io::Result as IoResult;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use tokio::io::ReadBuf;

/// An adapter that converts between `futures` and `tokio` I/O traits.
#[pin_project]
#[derive(Debug)]
pub struct TokioAdapter<S>(#[pin] pub S);

impl<S> TokioAdapter<S> {
    /// Create a new socket wrapper from the given socket.
    pub fn new(sock: S) -> Self {
        TokioAdapter(sock)
    }
}

/// futures -> tokio `AsyncRead` adapter.
impl<S: futures::AsyncRead> tokio::io::AsyncRead for TokioAdapter<S> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, cur: &mut ReadBuf) -> Poll<IoResult<()>> {
        let this = self.project().0;

        let buf = cur.as_slice_mut();
        let num = ready!(this.poll_read(cx, buf))?;
        cur.advance(num);

        Poll::Ready(Ok(()))
    }
}

/// tokio -> futures `AsyncRead` adapter.
impl<S: tokio::io::AsyncRead> futures::AsyncRead for TokioAdapter<S> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut [u8]) -> Poll<IoResult<usize>> {
        let this = self.project().0;

        let mut buf = ReadBuf::new(buf);
        ready!(this.poll_read(cx, &mut buf))?;
        let len = buf.filled().len();

        Poll::Ready(Ok(len))
    }
}

/// futures -> tokio `AsyncWrite` adapter.
impl<S: futures::AsyncWrite> tokio::io::AsyncWrite for TokioAdapter<S> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<IoResult<usize>> {
        self.project().0.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<IoResult<()>> {
        self.project().0.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<IoResult<()>> {
        self.project().0.poll_close(cx)
    }
}

/// tokio -> futures `AsyncWrite` adapter.
impl<S: tokio::io::AsyncWrite> futures::AsyncWrite for TokioAdapter<S> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<IoResult<usize>> {
        self.project().0.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<IoResult<()>> {
        self.project().0.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<IoResult<()>> {
        self.project().0.poll_shutdown(cx)
    }
}

trait AsSliceMut {
    fn as_slice_mut(&mut self) -> &mut [u8];
}

impl AsSliceMut for ReadBuf<'_> {
    fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe { &mut *(std::ptr::from_mut(self.unfilled_mut()) as *mut [u8]) }
    }
}
