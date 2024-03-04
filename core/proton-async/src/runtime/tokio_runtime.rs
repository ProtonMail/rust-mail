use crate::runtime::LocalTaskSetSpawn;
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

pub fn new_multi_thread_runtime(workers: usize) -> Result<tokio::runtime::Runtime, Box<dyn Error>> {
    let r = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(workers)
        .build()?;
    Ok(r)
}

#[derive(Default)]
#[pin_project::pin_project]
pub struct TokioLocalSet(#[pin] tokio::task::LocalSet);

impl TokioLocalSet {
    pub fn new() -> Self {
        Self(tokio::task::LocalSet::new())
    }
}

impl LocalTaskSetSpawn for TokioLocalSet {
    fn spawn_local<R: 'static, F: Future<Output = R> + 'static>(
        &self,
        f: F,
    ) -> crate::runtime::JoinHandle<R> {
        crate::runtime::JoinHandle::<R>::new(self.0.spawn_local(f))
    }
}

impl Future for TokioLocalSet {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll(cx)
    }
}
