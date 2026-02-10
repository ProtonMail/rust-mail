//! ## Timeout
//!
//! This module provides a layer that enforce timeouts on various operations.

use crate::Result;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::prelude::*;
use futures_timer::Delay;
use muon_proc::autoimpl;
use pin_project::pin_project;
use std::borrow::Borrow;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use thiserror::Error;

/// The timeout for all DNS-over-HTTPS queries.
pub const DNS_DOH_QUERY: Duration = Duration::from_secs(10);

/// The timeout for all DNS UDP request.
pub const DNS_UDP_QUERY: Duration = Duration::from_secs(5);

/// The TCP handshake timeout.
pub const TCP_CONNECT: Duration = Duration::from_secs(5);

/// The TLS handshake timeout.
pub const TLS_HANDSHAKE: Duration = Duration::from_secs(5);

/// Interval for the HTTP2 ping frame.
pub const HTTP_KEEPALIVE: Duration = Duration::from_secs(5);

/// Timeout when trying to establish an HTTP connection.
pub const HTTP_PROXY_CONNECT: Duration = Duration::from_secs(5);

/// A timeout error.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("operation timed out")]
pub struct Timeout;

/// Applies a timeout to a future.
#[autoimpl]
pub trait WithTimeout: Future + Sized {
    /// Wrap a future with a timeout of the given duration.
    fn with_timeout(self, t: impl Borrow<Duration>) -> TimeoutFut<Self> {
        TimeoutFut::new(self, t.borrow().to_owned(), None)
    }

    /// Wrap a future with a controllable timeout of the given duration.
    fn with_timeout_ctl(self, t: impl Borrow<Duration>, rx: TimeoutOpRx) -> TimeoutFut<Self> {
        TimeoutFut::new(self, t.borrow().to_owned(), Some(rx))
    }
}

/// A timeout controller.
#[derive(Debug, Clone)]
pub struct TimeoutCtl {
    tx: Arc<TimeoutOpTx>,
}

impl TimeoutCtl {
    /// Create a new timeout controller from a channel.
    pub fn new(tx: TimeoutOpTx) -> Self {
        Self { tx: Arc::new(tx) }
    }

    /// Pauses the timeout, preventing it from expiring until resumed.
    pub fn pause(&self) {
        self.tx.unbounded_send(TimeoutOp::Pause).ok();
    }

    /// Resumes the timeout, allowing it to expire after the remaining duration.
    pub fn resume(&self) {
        self.tx.unbounded_send(TimeoutOp::Resume).ok();
    }
}

/// A receiver of timeout operations.
pub type TimeoutOpRx = UnboundedReceiver<TimeoutOp>;

/// A sender of timeout operations.
pub type TimeoutOpTx = UnboundedSender<TimeoutOp>;

/// An operation that can be performed on a timeout future.
#[derive(Debug)]
pub enum TimeoutOp {
    /// Pause the timeout.
    Pause,

    /// Resume the timeout.
    Resume,
}

/// A future with a timeout.
#[pin_project]
#[derive(Debug)]
pub struct TimeoutFut<F> {
    #[pin]
    inner: F,

    #[pin]
    state: State,

    #[pin]
    rx: Option<TimeoutOpRx>,
}

impl<F: Future> TimeoutFut<F> {
    /// Wrap a future with a timeout of the given duration.
    pub fn new(inner: F, duration: Duration, rx: Option<TimeoutOpRx>) -> Self {
        let state = State::active(duration);

        Self { inner, state, rx }
    }
}

impl<F: Future> Future for TimeoutFut<F> {
    type Output = Result<F::Output, Timeout>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();

        if let Some(mut rx) = this.rx.as_pin_mut() {
            while let Poll::Ready(Some(op)) = rx.poll_next_unpin(cx) {
                match (this.state.as_mut().project(), op) {
                    (StateProj::Active { deadline, .. }, TimeoutOp::Pause) => {
                        *this.state = State::paused(*deadline);
                    }

                    (StateProj::Paused { remaining }, TimeoutOp::Resume) => {
                        *this.state = State::active(*remaining);
                    }

                    (StateProj::Active { .. }, TimeoutOp::Resume) => {
                        warn!("ignoring resume operation while active");
                    }

                    (StateProj::Paused { .. }, TimeoutOp::Pause) => {
                        warn!("ignoring pause operation while paused");
                    }
                }
            }
        }

        if let StateProj::Active { delay, .. } = this.state.as_mut().project() {
            if let Poll::Ready(()) = delay.poll(cx) {
                return Poll::Ready(Err(Timeout));
            }
        }

        this.inner.poll(cx).map(Ok)
    }
}

#[pin_project(project = StateProj)]
#[derive(Debug)]
enum State {
    Active {
        #[pin]
        delay: Delay,
        deadline: Instant,
    },

    Paused {
        remaining: Duration,
    },
}

impl State {
    fn active(remaining: Duration) -> Self {
        Self::Active {
            delay: Delay::new(remaining),
            deadline: Instant::now() + remaining,
        }
    }

    fn paused(deadline: Instant) -> Self {
        Self::Paused {
            remaining: deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::DurationExt;

    #[test]
    fn test_with_timeout() {
        futures::executor::block_on(async {
            let fut = Delay::new(1.s());
            let fut = fut.with_timeout(500.ms());
            assert_eq!(fut.await, Err(Timeout));

            let fut = Delay::new(500.ms());
            let fut = fut.with_timeout(1.s());
            assert_eq!(fut.await, Ok(()));
        });
    }
}
