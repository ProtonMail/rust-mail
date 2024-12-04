use crate::consts::CoreBundle;
use crate::services::proton::response_data::{ApiErrorInfo, HumanVerificationChallenge};

/// Extension for
/// [`APIErrorInfo`](`crate::services::proton::response_data::ApiErrorInfo`)
/// which handles human verification errors.
///
/// Human verification challenges can be returned at any time with 422 http status
/// code.
pub trait ApiErrorExtension {
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

impl ApiErrorExtension for ApiErrorInfo {
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
