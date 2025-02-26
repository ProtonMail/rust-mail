#![allow(clippy::unused_async)]

use crate::consts::CoreBundle;
use crate::services::proton::common::{AuthId, UserId};
use crate::services::proton::response_data::{ApiErrorInfo, HumanVerificationChallenge};
use async_trait::async_trait;
use derive_more::{Debug, Deref};
use muon::common::{BoxFut, Sender, SenderLayer};
use muon::{ProtonRequest, ProtonResponse, Result as MuonResult};
use std::sync::Arc;

/// Extension for
/// [`APIErrorInfo`](`crate::services::proton::response_data::ApiErrorInfo`)
/// which handles human verification errors.
///
/// Human verification challenges can be returned at any time with 422 http status
/// code.
pub trait ApiErrorInfoExt {
    /// Check whether the error is a human verification challenge.
    fn is_human_verification_challenge(&self) -> bool;

    /// Convert the
    fn to_human_verification_challenge(&self) -> Result<HumanVerificationChallenge, Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to deserialize human verification challenge: {0}")]
    Deserialization(#[from] serde_json::Error),
    #[error("Error is not human verification challenge")]
    NotAHumanVerificationChallenge,
    /// Human verification was indicated, but the data is missing.
    #[error("Missing human verification data")]
    MissingHumanVerificationData,
}

impl ApiErrorInfoExt for ApiErrorInfo {
    fn is_human_verification_challenge(&self) -> bool {
        self.code == CoreBundle::HumanVerificationRequired as u32
    }

    fn to_human_verification_challenge(&self) -> Result<HumanVerificationChallenge, Error> {
        if !self.is_human_verification_challenge() {
            return Err(Error::NotAHumanVerificationChallenge);
        }

        let Some(details) = self.details.clone() else {
            return Err(Error::MissingHumanVerificationData);
        };

        Ok(serde_json::from_value::<HumanVerificationChallenge>(
            details,
        )?)
    }
}

/// The payload of a human verification challenge.
#[derive(Debug, Clone)]
pub struct ChallengePayload {
    /// The ID of the user who is being challenged.
    pub user_id: UserId,

    /// The ID of the session in which the challenge is being issued.
    pub session_id: AuthId,

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
    async fn on_challenge(&self, payload: ChallengePayload, callback: ChallengeCallback);
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
    async fn on_challenge(&self, _: ChallengePayload, _: ChallengeCallback) {}
}
