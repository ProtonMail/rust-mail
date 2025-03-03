use std::{ops::Deref, sync::Arc};

use futures::TryFutureExt;
use proton_api_core::human_verification as hv;

/// An HTTP client capable of loading a human verification challenge.
#[derive(Debug, uniffi::Object)]
pub struct ChallengeLoader {
    inner: hv::ChallengeLoader,
}

#[uniffi_export]
impl ChallengeLoader {
    /// Handle a `GET` request, returning the response.
    ///
    /// This is a placeholder.
    pub async fn get(&self, url: &str) -> Result<String, String> {
        self.inner.get(url).map_err(|e| e.to_string()).await
    }
}

/// The payload of a human verification challenge.
#[derive(Debug, uniffi::Object)]
pub struct ChallengePayload {
    inner: hv::ChallengePayload,
}

#[uniffi_export]
impl ChallengePayload {
    /// The URL to load the challenge from.
    #[must_use]
    pub fn challenge_url(&self) -> String {
        self.inner.challenge_url.clone()
    }
}

/// The callback for a human verification challenge.
#[derive(Debug, uniffi::Object)]
pub struct ChallengeCallback {
    inner: hv::ChallengeCallback,
}

#[uniffi_export]
impl ChallengeCallback {
    /// Called when the challenge has been successfully completed.
    ///
    /// This submits the token to the server and returns any information that
    /// the server may have provided (e.g. cookies).
    pub async fn on_success(&self, token_type: String, token_code: String) {
        self.inner.on_success(token_type, token_code).await;
    }

    /// Called when the challenge has failed.
    pub async fn on_failed(&self) {
        self.inner.on_failed().await;
    }

    /// Called when the challenge has been cancelled.
    pub async fn on_cancelled(&self) {
        self.inner.on_cancelled().await;
    }
}

/// An interface by which human verification challenges can be handled.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait ChallengeNotifier: Send + Sync + 'static {
    async fn on_challenge(
        &self,
        loader: Arc<ChallengeLoader>,
        payload: Arc<ChallengePayload>,
        callback: Arc<ChallengeCallback>,
    );
}

#[async_trait::async_trait]
impl<T: ?Sized + ChallengeNotifier> ChallengeNotifier for Arc<T> {
    async fn on_challenge(
        &self,
        loader: Arc<ChallengeLoader>,
        payload: Arc<ChallengePayload>,
        callback: Arc<ChallengeCallback>,
    ) {
        self.deref().on_challenge(loader, payload, callback).await;
    }
}

/// Wraps a `ChallengeNotifier` to implement the core `ChallengeNotifier` trait.
pub(crate) struct ChallengeNotifierWrap<T> {
    inner: T,
}

impl<T> ChallengeNotifierWrap<T> {
    /// Create a new `ChallengeNotifierImpl`.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl<T: ChallengeNotifier> hv::ChallengeNotifier for ChallengeNotifierWrap<T> {
    async fn on_challenge(
        &self,
        loader: hv::ChallengeLoader,
        payload: hv::ChallengePayload,
        callback: hv::ChallengeCallback,
    ) {
        let loader = Arc::new(ChallengeLoader { inner: loader });
        let payload = Arc::new(ChallengePayload { inner: payload });
        let callback = Arc::new(ChallengeCallback { inner: callback });

        self.inner.on_challenge(loader, payload, callback).await;
    }
}
