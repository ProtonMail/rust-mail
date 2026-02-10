use crate::common::prelude::*;
use crate::rt::Dialer;
use crate::{ErrorKind, Result};
use async_trait::async_trait;
use futures::TryFutureExt;
use pin_project::pin_project;
use std::io::Result as IoResult;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use tokio::io::ReadBuf;
use tokio::net::TcpStream;

/// An async dialer backed by Tokio.
#[derive(Debug, Default)]
pub struct TokioDialer;

#[async_trait]
impl Dialer for TokioDialer {
    async fn dial(&self, addr: SocketAddr) -> Result<DynSocket> {
        trace!(?addr, "dialing host");

        let sock = TcpStream::connect(addr).map_err(ErrorKind::dial).await?;

        Ok(LocalSock(sock).into_dyn())
    }
}

#[pin_project]
struct LocalSock<S>(#[pin] S);

impl<S: tokio::io::AsyncRead> futures::AsyncRead for LocalSock<S> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut [u8]) -> Poll<IoResult<usize>> {
        let mut buf = ReadBuf::new(buf);

        ready!(self.project().0.poll_read(cx, &mut buf))?;

        Poll::Ready(Ok(buf.filled().len()))
    }
}

impl<S: tokio::io::AsyncWrite> futures::AsyncWrite for LocalSock<S> {
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
