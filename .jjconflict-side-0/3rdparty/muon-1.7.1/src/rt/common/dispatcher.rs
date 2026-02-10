use crate::rt::Spawner;
use derive_more::Debug;
use futures::future::BoxFuture;
use futures::prelude::*;
use muon_proc::autoimpl;
use pin_project::pin_project;
use std::borrow::Borrow;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};

/// Create a new dispatcher and its driver.
///
/// The dispatcher and driver work together to execute futures concurrently.
/// The dispatcher spawns futures onto a queue and the driver polls them.
#[must_use]
pub fn dispatcher() -> (Dispatcher, Driver) {
    let state = State::new();
    let driver = Driver {
        state: state.clone(),
    };
    let dispatcher = Dispatcher {
        state: state.clone(),
    };

    (dispatcher, driver)
}

/// A dispatcher.
#[derive(Debug, Clone)]
pub struct Dispatcher {
    state: Arc<State>,
}

impl Spawner for Dispatcher {
    fn spawn(&self, fut: BoxFuture<'static, ()>) {
        if let Some(waker) = self.state.push(Box::pin(fut)) {
            waker.wake();
        }
    }
}

impl Drop for Dispatcher {
    fn drop(&mut self) {
        trace!("dropping dispatcher");
    }
}

/// A driver that drives futures spawned by the dispatcher.
#[derive(Debug, Clone)]
pub struct Driver {
    state: Arc<State>,
}

impl Driver {
    fn poll(&self, cx: &mut Context) -> Poll<()> {
        let mut queue = self.state.take_queue(cx);

        queue.retain_mut(|f| f.poll_unpin(cx).is_pending());

        self.state.push_queue(queue);

        Poll::Pending
    }
}

impl Future for Driver {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.as_ref().poll(cx)
    }
}

impl Future for &Driver {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.as_ref().poll(cx)
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        trace!("dropping driver");
    }
}

#[derive(Debug, Default)]
struct State {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    #[debug(skip)]
    queue: Option<Vec<BoxFuture<'static, ()>>>,
    waker: Option<Waker>,
}

impl State {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn push(&self, fut: BoxFuture<'static, ()>) -> Option<Waker> {
        self.inner.with_lock(|mut s| {
            s.queue.get_or_insert_with(Vec::new).push(fut);
            s.waker.take()
        })
    }

    fn take_queue(&self, cx: &Context) -> Vec<BoxFuture<'static, ()>> {
        self.inner.with_lock(|mut s| {
            s.waker = Some(cx.waker().to_owned());
            s.queue.take().unwrap_or_default()
        })
    }

    fn push_queue(&self, queue: Vec<BoxFuture<'static, ()>>) {
        self.inner.with_lock(|mut s| {
            s.queue.get_or_insert_with(Vec::new).extend(queue);
        });
    }
}

impl Drop for State {
    fn drop(&mut self) {
        trace!("dropping state");
    }
}

/// Links a future to a driver; when the future is polled, so is the driver.
#[autoimpl]
pub trait PollWith: Future + Sized {
    /// Poll this future alongside the given driver.
    fn poll_with(self, driver: impl Borrow<Driver>) -> PollWithFut<Self> {
        PollWithFut {
            future: self,
            driver: driver.borrow().to_owned(),
        }
    }
}

/// A future that polls a future alongside a driver.
#[pin_project]
#[derive(Debug)]
pub struct PollWithFut<F> {
    #[pin]
    future: F,

    #[pin]
    driver: Driver,
}

impl<F: Future> Future for PollWithFut<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.project();

        match Future::poll(this.driver, cx) {
            Poll::Ready(()) => unreachable!("unexpected driver completion"),
            Poll::Pending => this.future.poll(cx),
        }
    }
}

/// A trait for types that can be locked.
trait WithLock<'a, T: ?Sized> {
    /// The guard type returned by the lock.
    type Guard;

    /// Lock the value and call the given function with the guard.
    fn with_lock<U>(&'a self, f: impl FnOnce(Self::Guard) -> U) -> U;
}

impl<'a, T: ?Sized + 'a> WithLock<'a, T> for Mutex<T> {
    type Guard = MutexGuard<'a, T>;

    fn with_lock<U>(&'a self, f: impl FnOnce(Self::Guard) -> U) -> U {
        if let Ok(guard) = self.lock() {
            f(guard)
        } else {
            panic!("lock poisoned")
        }
    }
}
