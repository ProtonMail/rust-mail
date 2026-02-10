use async_trait::async_trait;
use http::{Request, Response};
use itertools::Itertools;
use muon_proc::autoimpl;
use std::error::Error;

pub fn is_retryable(e: &hyper::Error) -> bool {
    if let Some(e) = e.source().and_then(|e| e.downcast_ref::<h2::Error>()) {
        warn!("h2 error: {}", error_stack(e));

        if e.is_remote() {
            if e.is_go_away() && e.reason() == Some(h2::Reason::NO_ERROR) {
                return true;
            }

            if e.is_reset() && e.reason() == Some(h2::Reason::REFUSED_STREAM) {
                return true;
            }
        }
    }

    if e.is_closed() {
        warn!("sender channel was closed");
        return true;
    }

    if e.is_timeout() {
        warn!("network operation timed out");
        return true;
    }

    false
}

fn error_stack(err: &(dyn Error + 'static)) -> String {
    let mut stack = Vec::new();
    let mut cause = Some(err);

    while let Some(e) = cause {
        stack.push(e);
        cause = e.source();
    }

    stack.into_iter().join(": ")
}

#[async_trait]
pub trait ReadySend<B> {
    async fn ready_send(&self, req: Request<B>) -> hyper::Result<Response<Vec<u8>>>;
}

#[async_trait]
pub trait ReadySendMut<B> {
    async fn ready_send_mut(&mut self, req: Request<B>) -> hyper::Result<Response<Vec<u8>>>;
}

#[autoimpl]
#[async_trait]
pub trait SendWith<B>: Into<Request<B>> + Sized {
    async fn send_with<T>(self, sender: &T) -> hyper::Result<Response<Vec<u8>>>
    where
        T: ReadySend<B> + Send + Sync,
    {
        sender.ready_send(self.into()).await
    }

    async fn send_with_mut<T>(self, sender: &mut T) -> hyper::Result<Response<Vec<u8>>>
    where
        T: ReadySendMut<B> + Send,
    {
        sender.ready_send_mut(self.into()).await
    }
}

pub fn fmt_req<B>(req: &Request<B>) -> String {
    format!("{} {}", req.method(), req.uri())
}
