use async_trait::async_trait;
use derive_more::Debug;
use std::{ops::Deref, sync::Arc};
use url::Url;

use crate::services::proton::prelude::HumanVerificationChallenge;

/// A dynamic human verification notifier.
pub type DynChallengeNotifier = Arc<dyn ChallengeNotifier>;

/// The payload of a human verification challenge.
#[derive(Debug, Clone)]
pub struct ChallengePayload {
    pub token: String,
    pub methods: Vec<String>,
    pub description: String,
    pub expires_at: u64,
    pub web_url: Url,
}

impl ChallengePayload {
    /// The URL base.
    #[must_use]
    pub fn base(&self) -> String {
        self.web_url.origin().ascii_serialization()
    }

    /// The URL path.
    #[must_use]
    pub fn path(&self) -> &str {
        self.web_url.path()
    }

    /// The query parameters of the URL.
    #[must_use]
    pub fn query(&self) -> Vec<(String, Option<String>)> {
        self.web_url
            .query_pairs()
            .map(|(k, v)| {
                if v.is_empty() {
                    (k.into_owned(), None)
                } else {
                    (k.into_owned(), Some(v.into_owned()))
                }
            })
            .collect()
    }
}

impl TryFrom<HumanVerificationChallenge> for ChallengePayload {
    type Error = url::ParseError;

    fn try_from(challenge: HumanVerificationChallenge) -> Result<Self, Self::Error> {
        Ok(Self {
            token: challenge.human_verification_token,
            methods: challenge.human_verification_methods,
            description: challenge.description,
            expires_at: challenge.expires_at,
            web_url: challenge.web_url.parse()?,
        })
    }
}

/// The result of a challenge notification.
#[derive(Debug)]
pub enum ChallengeResponse {
    Success { token: String, ttype: String },
    Failure,
    Cancelled,
}

impl ChallengeResponse {
    pub fn success(token: impl AsRef<str>, ttype: impl AsRef<str>) -> Self {
        Self::Success {
            token: token.as_ref().to_owned(),
            ttype: ttype.as_ref().to_owned(),
        }
    }
}

/// An interface by which human verification challenges can be handled.
#[async_trait]
pub trait ChallengeNotifier: Send + Sync {
    async fn on_challenge(&self, payload: ChallengePayload) -> ChallengeResponse;
}

#[async_trait]
impl<T: ?Sized> ChallengeNotifier for Arc<T>
where
    T: ChallengeNotifier,
{
    async fn on_challenge(&self, payload: ChallengePayload) -> ChallengeResponse {
        self.deref().on_challenge(payload).await
    }
}

pub(crate) struct FailNotifier;

impl FailNotifier {
    #[must_use]
    pub fn arced() -> DynChallengeNotifier {
        Arc::new(Self)
    }
}

#[async_trait]
impl ChallengeNotifier for FailNotifier {
    async fn on_challenge(&self, _: ChallengePayload) -> ChallengeResponse {
        ChallengeResponse::Failure
    }
}
