use crate::common::DynSocket;
use crate::rt::{DynSpawner, Spawner, SpawnerExt};
use futures::prelude::*;
use hyper::rt::{Executor, Read, ReadBuf, ReadBufCursor as Cursor, Write};
use pin_project::pin_project;
use std::io::Result as IoResult;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

if_rt_async! {{
    struct HyperSleep(futures_timer::Delay);
    impl HyperSleep {
        fn sleep(duration: std::time::Duration) -> Self {
            Self(futures_timer::Delay::new(duration))
        }

        fn sleep_until(deadline: std::time::Instant) -> Self {
            let duration = deadline - std::time::Instant::now();
            Self::sleep(duration)
        }
    }
    impl Future for HyperSleep {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            self.0.poll_unpin(cx)
        }
    }
} else if_rt_tokio! {{
    struct HyperSleep(Pin<Box<tokio::time::Sleep>>);
    impl HyperSleep{
        fn sleep(duration: std::time::Duration) -> Self {
            let sleep = tokio::time::sleep(duration);
            Self(Box::pin(sleep))
        }
        fn sleep_until(deadline: std::time::Instant) -> Self {
            let sleep = tokio::time::sleep_until(deadline.into());
            Self(Box::pin(sleep))
        }
    }
    impl Future for HyperSleep {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            self.0.as_mut().poll(cx)
        }
    }
} else {
    compile_error!("a runtime must be enabled")
}}}

/// A timer that can be injected into Hyper
pub(crate) struct HyperTimer;
impl hyper::rt::Sleep for HyperSleep {}
impl hyper::rt::Timer for HyperTimer {
    fn sleep(&self, duration: std::time::Duration) -> std::pin::Pin<Box<dyn hyper::rt::Sleep>> {
        Box::pin(HyperSleep::sleep(duration))
    }

    fn sleep_until(
        &self,
        deadline: std::time::Instant,
    ) -> std::pin::Pin<Box<dyn hyper::rt::Sleep>> {
        Box::pin(HyperSleep::sleep_until(deadline))
    }
}

/// An adapter that converts between `futures` and `hyper` I/O traits.
#[pin_project]
#[derive(Debug)]
pub struct HyperIo<S = DynSocket>(#[pin] pub S);

impl<S: AsyncRead> Read for HyperIo<S> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, mut cur: Cursor) -> Poll<IoResult<()>> {
        let this = self.project().0;

        let buf = cur.as_slice_mut();
        let num = ready!(this.poll_read(cx, buf))?;
        unsafe { cur.advance(num) };

        Poll::Ready(Ok(()))
    }
}

impl<S: Read> AsyncRead for HyperIo<S> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut [u8]) -> Poll<IoResult<usize>> {
        let this = self.project().0;

        let mut buf = ReadBuf::new(buf);
        ready!(this.poll_read(cx, buf.unfilled()))?;
        let num = buf.filled().len();

        Poll::Ready(Ok(num))
    }
}

impl<S: AsyncWrite> Write for HyperIo<S> {
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

impl<S: Write> AsyncWrite for HyperIo<S> {
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

impl AsSliceMut for Cursor<'_> {
    fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe {
            let this = self.as_mut();
            let this = std::ptr::from_mut(this);
            let this = this as *mut [u8];

            &mut *this
        }
    }
}

#[derive(Clone)]
pub struct HyperRt<S = DynSpawner>(pub S);

// TODO: Can we reduce the bounds on the future here?
impl<S: Spawner, F: Future> Executor<F> for HyperRt<S>
where
    F: Send + 'static,
    F::Output: Send,
{
    fn execute(&self, fut: F) {
        trace!("spawning hyper future");

        self.0.spawn_any(fut.map(|_| ()));
    }
}
