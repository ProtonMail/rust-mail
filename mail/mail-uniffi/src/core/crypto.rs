use proton_crypto_account::proton_crypto::crypto::{
    VerificationError, VerificationInformation, VerificationResult,
};

#[derive(Debug, Clone, Eq, PartialEq, Copy, uniffi::Enum)]
pub enum SignatureVerification {
    /// Successfully verified the signature
    Ok,
    /// No signature found
    NotSigned,
    /// No matching key found.
    NoVerifier,
    /// Signature verification failure.
    Failed,
    /// Signature context did not match verification context.
    BadContext,
    /// Unknown error occurred.
    RuntimeError,
}

/// Represent the result of a `OpenPGP` signature verification.
#[derive(Debug, Clone, uniffi::Object)]
pub struct SignatureVerificationResult {
    result_state: SignatureVerification,
    error_info: Option<String>,
    signature_info: Option<VerificationInformation>,
}

#[uniffi_export]
impl SignatureVerificationResult {
    /// The result of the signature verification with an enum type.
    #[must_use]
    pub fn verification_result(&self) -> SignatureVerification {
        self.result_state
    }

    /// Returns more info about the signature verification error.
    ///
    /// If the verification was successful there is no message.
    #[must_use]
    pub fn error_info(&self) -> Option<String> {
        self.error_info.clone()
    }

    /// Returns the key id of the key the signature was created with if any.
    #[must_use]
    pub fn signature_key_id(&self) -> Option<u64> {
        self.signature_info.as_ref().map(|info| info.key_id.0)
    }

    /// Returns the creation time of the signature.
    #[must_use]
    pub fn signature_creation_time(&self) -> Option<u64> {
        self.signature_info
            .as_ref()
            .map(|info| info.signature_creation_time.value())
    }
}

impl From<VerificationResult> for SignatureVerificationResult {
    fn from(value: VerificationResult) -> Self {
        match value {
            Ok(info) => Self {
                result_state: SignatureVerification::Ok,
                error_info: None,
                signature_info: Some(info),
            },
            Err(err) => {
                let (result_state, error_info, info_opt) = match err {
                    VerificationError::NotSigned(err) => {
                        (SignatureVerification::NotSigned, err.to_string(), None)
                    }
                    VerificationError::NoVerifier(err) => {
                        (SignatureVerification::NoVerifier, err.to_string(), None)
                    }
                    VerificationError::Failed(info, err) => {
                        (SignatureVerification::Failed, err.to_string(), Some(info))
                    }
                    VerificationError::BadContext(info, err) => (
                        SignatureVerification::BadContext,
                        err.to_string(),
                        Some(info),
                    ),
                    VerificationError::RuntimeError(err) => {
                        (SignatureVerification::RuntimeError, err.to_string(), None)
                    }
                };
                Self {
                    result_state,
                    error_info: Some(error_info),
                    signature_info: info_opt,
                }
            }
        }
    }
}

#[uniffi::export]
#[must_use]
pub fn generate_csp_nonce() -> String {
    proton_core_common::utils::generate_csp_nonce()
}
