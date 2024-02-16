use crate::runtime::{JoinHandle, LocalTaskSet};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub type Runtime = tokio::runtime::Runtime;

pub fn new_thread_local_runtime() -> Result<tokio::runtime::Runtime, Box<dyn Error>> {
    let r = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    Ok(r)
}

pub fn new_multi_thread_runtime() -> Result<tokio::runtime::Runtime, Box<dyn Error>> {
    let r = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    Ok(r)
}

#[pin_project::pin_project]
pub struct TokioJoinHandle<R>(#[pin] tokio::task::JoinHandle<R>);

impl<R: Send + 'static> TokioJoinHandle<R> {
    pub fn new<F: Future<Output = R> + Send + 'static>(
        runtime: &tokio::runtime::Runtime,
        f: F,
    ) -> Self {
        TokioJoinHandle(runtime.spawn(f))
    }
}

impl<R> Future for TokioJoinHandle<R> {
    type Output = Result<R, Box<dyn Error>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project()
            .0
            .poll(cx)
            .map(|r| r.map_err(|e| -> Box<dyn Error> { Box::new(e) }))
    }
}

#[pin_project::pin_project]
pub struct TokioLocalSet(#[pin] tokio::task::LocalSet);

impl TokioLocalSet {
    pub fn new() -> Self {
        Self(tokio::task::LocalSet::new())
    }
}

impl<R: 'static> JoinHandle<R> for TokioJoinHandle<R> {}

impl LocalTaskSet for TokioLocalSet {
    fn spawn_local<R: 'static, F: Future<Output = R> + 'static>(&self, f: F) -> impl JoinHandle<R> {
        TokioJoinHandle(self.0.spawn_local(f))
    }
}

impl Future for TokioLocalSet {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll(cx)
    }
}

pub fn spawn<R: Send + 'static, F: Future<Output = R> + Send + 'static>(
    f: F,
) -> TokioJoinHandle<R> {
    #[cfg(feature = "tokio-runtime")]
    TokioJoinHandle(tokio::spawn(f))
}
