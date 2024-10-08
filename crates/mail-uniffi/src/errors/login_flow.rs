use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::mail::{LoginFlow, MailUserSession};
use itertools::Itertools;
use proton_api_core::services::proton::common::HumanVerificationType as RealHumanVerificationType;
use proton_api_core::services::proton::response_data::HumanVerificationChallenge as RealHumanVerificationChallenge;
use proton_mail_common::errors::login_flow::Reason as RealReason;
use proton_mail_common::errors::login_flow::UserLoginFlowError as RealLoginFlowError;
use std::sync::Arc;

/// Representation of a `Result<(), Error>` for the clients
#[derive(Debug, uniffi::Enum)]
pub enum UserLoginFlowVoidResult {
    Ok,
    Error(UserLoginFlowError),
}

impl<T, E: Into<RealLoginFlowError>> From<Result<T, E>> for UserLoginFlowVoidResult {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(_) => Self::Ok,
            Err(error) => Self::Error(error.into().into()),
        }
    }
}

impl From<()> for UserLoginFlowVoidResult {
    fn from(_value: ()) -> Self {
        Self::Ok
    }
}

impl From<RealLoginFlowError> for UserLoginFlowVoidResult {
    fn from(error: RealLoginFlowError) -> Self {
        Self::Error(error.into())
    }
}

#[derive(Debug, uniffi::Enum)]
pub enum UserLoginFlowError {
    InvalidAction(LoginReason),
    ServerError(UserApiServiceError),
    Network,
    Unexpected(UnexpectedError),
}

impl From<RealLoginFlowError> for UserLoginFlowError {
    fn from(error: RealLoginFlowError) -> Self {
        match error {
            RealLoginFlowError::Reason(reason) => Self::InvalidAction(LoginReason::from(reason)),
            RealLoginFlowError::ServerError(user_api_service_error) => {
                Self::ServerError(UserApiServiceError::from(user_api_service_error))
            }
            RealLoginFlowError::Network => Self::Network,
            RealLoginFlowError::Unexpected(unexpected) => {
                Self::Unexpected(UnexpectedError::from(unexpected))
            }
        }
    }
}

/// Reason for invalid Action
#[derive(Debug, uniffi::Enum)]
pub enum LoginReason {
    HumanVerificationChallenge(HumanChallenge),
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

impl From<RealReason> for LoginReason {
    fn from(reason: RealReason) -> Self {
        match reason {
            RealReason::HumanVerificationChallenge(challenge) => {
                Self::HumanVerificationChallenge(challenge.into())
            }
            RealReason::InvalidCredentials => Self::InvalidCredentials,
            RealReason::UnsupportedTfa => Self::UnsupportedTfa,
            RealReason::CantUnlockUserKey => Self::CantUnlockUserKey,
        }
    }
}

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

macro_rules! result_builder {
    ($name:ident, $type:ty) => {
        #[allow(clippy::large_enum_variant)]
        #[derive(uniffi::Enum)]
        pub enum $name {
            Ok($type),
            Error(UserLoginFlowError),
        }

        impl<E: Into<RealLoginFlowError>> From<Result<$type, E>> for $name {
            fn from(value: Result<$type, E>) -> Self {
                match value {
                    Ok(value) => Self::Ok(value),
                    Err(error) => Self::Error(error.into().into()),
                }
            }
        }

        impl From<$type> for $name {
            fn from(value: $type) -> Self {
                Self::Ok(value)
            }
        }

        impl From<RealLoginFlowError> for $name {
            fn from(error: RealLoginFlowError) -> Self {
                Self::Error(error.into())
            }
        }
    };
}

result_builder!(UserLoginFlowArcMailUserSessionResult, Arc<MailUserSession>);
result_builder!(UserLoginFlowArcLoginFlowResult, Arc<LoginFlow>);
result_builder!(UserLoginFlowStringResult, String);
