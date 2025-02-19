//! This library provides Business Unit agnostic code for decrypting and veryfing Push Notifications.
//!  

use proton_crypto_account::proton_crypto::{
    crypto::{
        AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerificationError,
        VerificationResult, VerifiedData, Verifier, VerifierSync,
    },
    CryptoError, CryptoInfoError,
};
use serde::Deserialize;

// re-export crypto crate;
pub use proton_crypto_account::proton_crypto;

// re-export account crate;
pub use proton_crypto_account;

/// Allows for lazy notification signature verification
///
pub struct VerifiableNotification {
    decrypted_row: Box<[u8]>,
    signatures: Box<[u8]>,
}

impl VerifiableNotification {
    #[must_use]
    fn new(row: Vec<u8>, signatures: Vec<u8>) -> Self {
        Self {
            decrypted_row: row.into_boxed_slice(),
            signatures: signatures.into_boxed_slice(),
        }
    }

    /// Verifies the message by checking the signature
    ///
    pub fn verify_signature<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    ) -> VerificationResult {
        if self.signatures.is_empty() {
            return Err(VerificationError::NotSigned(
                CryptoInfoError::new("No signature found").into(),
            ));
        }

        if verification_keys.is_empty() {
            return Err(VerificationError::NoVerifier(
                CryptoInfoError::new("No verification key provided").into(),
            ));
        }

        pgp_provider
            .new_verifier()
            .with_verification_key_refs(verification_keys)
            .verify_detached(&self.decrypted_row, &self.signatures, DataEncoding::Bytes)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("Failed to decrypt the notification: {0}")]
    Decryption(CryptoError),

    #[error("Failed to deserialize notification: {0}")]
    Deserialization(serde_json::Error),
}

/// Notification stored as generic JSON object.
/// Because push notifications are BU independent, we do not assume its content at the
/// decryption stage
///
#[derive(Deserialize, Clone, Debug)]
pub struct DecryptedNotification {
    /// Decrypted notification payload
    pub data: serde_json::Value,

    /// Notification kind
    #[serde(rename = "type")]
    pub kind: NotificationKind,
}

/// Notification kind
///
#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    /// New email received
    Email,
    /// Used for anti abuse - new sign to your account has happened, opens a web url
    OpenUrl,
}

/// Notification encrypted by PGP key. Can be decrypted by [`DecryptableNotification::decrypt`].
///
pub trait GettablePGPNotification {
    /// Encrypted slice of bytes
    ///
    fn pgp_notification(&self) -> &[u8];
}

pub trait DecryptableNotification: GettablePGPNotification {
    /// Decrypt the notification
    ///
    /// Note, that function does not verify if the signature is correct. Instead, it returns
    /// [`VerifiableNotification`] which can be used for it.
    ///
    fn decrypt<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        decryption_keys: &[impl AsRef<T::PrivateKey>],
    ) -> Result<(DecryptedNotification, VerifiableNotification), NotificationError> {
        let data = self.pgp_notification();
        let decrypted_notification = pgp_provider
            .new_decryptor()
            .with_decryption_key_refs(decryption_keys)
            .decrypt(data, DataEncoding::Armor)
            .map_err(NotificationError::Decryption)?;

        let signatures = decrypted_notification.signatures().unwrap_or_default();
        let raw_notification_data = decrypted_notification.into_vec();
        let notification = serde_json::from_slice(&raw_notification_data)
            .map_err(NotificationError::Deserialization)?;

        let verifier = VerifiableNotification::new(raw_notification_data, signatures);

        Ok((notification, verifier))
    }
}
