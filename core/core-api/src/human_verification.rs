#![allow(clippy::unused_async)]

use crate::service::ApiServiceResult;
use async_trait::async_trait;
use derive_more::{Debug, Deref};
use muon::common::{BoxFut, Sender, SenderLayer};
use muon::{ProtonRequest, ProtonResponse, Result as MuonResult};
use std::sync::Arc;

/// An HTTP client capable of loading a human verification challenge.
#[derive(Debug)]
pub struct ChallengeLoader {
    // ...
}

impl ChallengeLoader {
    /// Handle a `GET` request, returning the response.
    ///
    /// This is a placeholder.
    pub async fn get(&self, _url: &str) -> ApiServiceResult<String> {
        todo!()
    }
}

/// The payload of a human verification challenge.
#[derive(Debug, Clone)]
pub struct ChallengePayload {
    /// The URL to load the challenge from.
    pub challenge_url: String,
}

/// The callback for a human verification challenge.
#[derive(Debug)]
pub struct ChallengeCallback {
    // ...
}

impl ChallengeCallback {
    /// Called when the challenge has been successfully completed.
    ///
    /// This submits the token to the server and returns any information that
    /// the server may have provided (e.g. cookies).
    pub async fn on_success(&self, _token_type: String, _token_code: String) {
        todo!()
    }

    /// Called when the challenge has failed.
    pub async fn on_failed(&self) {
        todo!()
    }

    /// Called when the challenge has been cancelled.
    pub async fn on_cancelled(&self) {
        todo!()
    }
}

/// An interface by which human verification challenges can be handled.
///
/// This is a placeholder for now and will be expanded in the future.
#[async_trait]
pub trait ChallengeNotifier: Send + Sync + 'static {
    async fn on_challenge(
        &self,
        loader: ChallengeLoader,
        payload: ChallengePayload,
        callback: ChallengeCallback,
    );
}

/// A type that holds registered [`ChallengeNotifier`]s.
///
/// This is a placeholder for now and will be expanded in the future.
#[must_use]
#[derive(Debug, Clone)]
#[debug("ChallengeObserver")]
pub struct ChallengeObserver {
    #[allow(unused)]
    notifier: Arc<dyn ChallengeNotifier>,
}

impl ChallengeObserver {
    pub fn new(notifier: impl ChallengeNotifier) -> Self {
        Self {
            notifier: Arc::new(notifier),
        }
    }
}

impl Default for ChallengeObserver {
    fn default() -> Self {
        Self {
            notifier: Arc::new(NoopNotifier),
        }
    }
}

/// A type that wraps a [`ChallengeObserver`] and to implement the [`SenderLayer`] trait.
#[derive(Debug, Deref)]
pub struct ChallengeObserverLayer(ChallengeObserver);

impl ChallengeObserverLayer {
    #[must_use]
    pub fn new(observer: ChallengeObserver) -> Self {
        Self(observer)
    }

    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        inner.send(req).await
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for ChallengeObserverLayer {
    fn on_send<'a: 'fut, 'fut>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'fut, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}

struct NoopNotifier;

#[async_trait]
impl ChallengeNotifier for NoopNotifier {
    async fn on_challenge(&self, _: ChallengeLoader, _: ChallengePayload, _: ChallengeCallback) {}
}
