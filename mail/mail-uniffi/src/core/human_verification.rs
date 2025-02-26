use std::sync::Arc;

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
    /// The ID of the user who is being challenged.
    #[must_use]
    pub fn user_id(&self) -> String {
        self.inner.user_id.clone().into_inner()
    }

    /// The ID of the session in which the challenge is being issued.
    #[must_use]
    pub fn session_id(&self) -> String {
        self.inner.session_id.clone().into_inner()
    }

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
pub trait ChallengeNotifier: Send + Sync {
    async fn on_challenge(
        &self,
        loader: Arc<ChallengeLoader>,
        payload: Arc<ChallengePayload>,
        callback: Arc<ChallengeCallback>,
    );
}
