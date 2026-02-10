//! This module adds utility logic for the "Encrypt-to-Ourside" (EO) feature
//! with password-based protection in mail.
//!
//! EO encryption requires the client to use a password to generate
//! SRP material and a challenge to be shared with the server.
use std::string::FromUtf8Error;

use base64::{Engine, prelude::BASE64_STANDARD};
use proton_crypto_account::proton_crypto::{
    CryptoError,
    crypto::{DataEncoding, Encryptor, EncryptorSync, PGPProviderSync},
    generate_secure_random_bytes,
    srp::{ClientVerifier, SRPProvider},
};
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum EoError {
    #[error("Failed to encrypt the EO challenge with passphrase: {0}")]
    Encryption(CryptoError),
    #[error("Failed to generate EO verifier: {0}")]
    Srp(CryptoError),
    #[error("Failed to decode message body to an UTF-8 string: {0}")]
    StringEncode(#[from] FromUtf8Error),
}

/// Represents a EO challenge that is provided to the server
/// when sending password encrypted messages.
pub struct Challenge {
    /// Base-64 encoded random token.
    pub token: Zeroizing<String>,

    /// Armored `OpenPGP` message containing the encrypted token.
    pub enc_token: String,

    /// The SRP verifier.
    pub verifier: ClientVerifier,
}

impl Challenge {
    /// Generates an EO encrypt-to-outside srp verifier and challenge.
    ///
    /// `srp_modulus` is the modulus for SRP and can be fetched from the BE.
    pub fn generate<P, S>(
        pgp: &P,
        srp: &S,
        password: &str,
        srp_modulus: &str,
    ) -> Result<Self, EoError>
    where
        P: PGPProviderSync,
        S: SRPProvider,
    {
        let challenge: Zeroizing<[u8; 32]> = Zeroizing::new(generate_secure_random_bytes());
        let token = Zeroizing::new(BASE64_STANDARD.encode(challenge));

        let enc_token = pgp
            .new_encryptor()
            .with_passphrase(password)
            .with_utf8()
            .encrypt_raw(token.as_bytes(), DataEncoding::Armor)
            .map_err(EoError::Encryption)
            .map(String::from_utf8)??;

        let verifier = srp
            .generate_client_verifier(password, srp_modulus)
            .map_err(EoError::Srp)?;

        Ok(Self {
            token,
            enc_token,
            verifier,
        })
    }
}
