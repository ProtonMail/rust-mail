use itertools::Itertools;
use proton_api_core::services::proton::common::HumanVerificationType as RealHumanVerificationType;
use proton_api_core::services::proton::response_data::HumanVerificationChallenge as RealHumanVerificationChallenge;
use tracing::error;

/// Information for the human verification challenge.
#[derive(Debug, uniffi::Record)]
pub struct HumanChallenge {
    /// Types of supported verification.
    pub method: Vec<HumanVerificationType>,
    /// Token for the verification request.
    pub token: String,
}

impl From<RealHumanVerificationChallenge> for HumanChallenge {
    fn from(value: RealHumanVerificationChallenge) -> Self {
        error!("HumanChallenge from {value:?}");
        Self {
            method: value.methods.into_iter().map_into().collect(),
            token: value.token,
        }
    }
}

/// Human verification type returned by the API.
#[derive(Debug, uniffi::Enum)]
pub enum HumanVerificationType {
    /// User needs to solve a Captcha, use [`crate::captcha_get`] to retrieve the token, solve in a web
    /// browser/view and retrieve the token posted via an `HVCaptchaMessage`.
    Captcha,

    /// User needs to verify via a token send via an email. Note: Request for this
    /// verification is not yet implemented.
    Email,

    /// User needs to verify via a token send via sms. Note: Request for this verification is not
    /// yet implemented.
    Sms,
}

impl From<RealHumanVerificationType> for HumanVerificationType {
    fn from(value: RealHumanVerificationType) -> Self {
        match value {
            RealHumanVerificationType::Captcha => Self::Captcha,
            RealHumanVerificationType::Email => Self::Email,
            RealHumanVerificationType::Sms => Self::Sms,
        }
    }
}
