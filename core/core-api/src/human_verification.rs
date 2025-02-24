use crate::consts::CoreBundle;
use crate::services::proton::response_data::{ApiErrorInfo, HumanVerificationChallenge};
use derive_more::Deref;
use muon::common::{BoxFut, Sender, SenderLayer};
use muon::{ProtonRequest, ProtonResponse, Result as MuonResult};

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

#[derive(Debug, Default, Clone)]
pub struct ChallengeObserver {}

impl ChallengeObserver {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug, Deref)]
pub struct ChallengeObserverLayer(ChallengeObserver);

impl ChallengeObserverLayer {
    pub fn new(observer: ChallengeObserver) -> Self {
        Self(observer)
    }
}

impl ChallengeObserverLayer {
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
