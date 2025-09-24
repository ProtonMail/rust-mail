use crate::common::BoxFut;
use crate::rt::Spawner;

/// An async spawner backed by Tokio.
#[must_use]
#[derive(Debug)]
pub struct TokioSpawner;

impl Spawner for TokioSpawner {
    fn spawn(&self, fut: BoxFut<'static, ()>) {
        tokio::spawn(fut);
    }
}
