use std::str::FromStr;

use base64::{prelude::BASE64_STANDARD as BASE_64, DecodeError, Engine as _};
use proton_crypto_account::proton_crypto::{
    crypto::{PGPProviderSync, SessionKey, SessionKeyAlgorithm},
    CryptoError,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::MessageError;

crate::string_id! {
    /// Key packet represents an encrypted message key with an `OpenPGP` key.
    ///
    /// The keys packet is represented as a base64 encoded string.
    /// Note that a key packet contains encrypted data and, thus, is not sensitive.
    KeyPacket
}

impl KeyPacket {
    /// Creates a new key packet from raw bytes.
    pub(crate) fn new_from_bytes(key_packets: &[u8]) -> Self {
        KeyPacket(BASE_64.encode(key_packets))
    }

    /// Decodes the key packet to raw bytes.
    pub fn decode(&self) -> Result<Vec<u8>, DecodeError> {
        BASE_64.decode(&self.0).map_err(Into::into)
    }
}

crate::string_id! {
    /// Exposed secret message key encoded as base64 string.
    ///
    /// The message key is a symmetric `OpenPGP` session key used
    /// for symmetric encryption.
    /// Its memory is zeroed after drop. Note that this
    /// type should be treated as a sensible secret.
    ///
    /// For example, the session keys has to be exposed to the backend
    /// if a email package is sent to a recipient with cleartext only.
    SessionKeyExposed
}

impl ZeroizeOnDrop for SessionKeyExposed {}

impl Drop for SessionKeyExposed {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// A secret symmetric message key that might be revealed to the backend.
///
/// Is wiped from memory on drop.
#[derive(Clone, Eq, PartialEq, Zeroize, ZeroizeOnDrop)]
#[allow(clippy::module_name_repetitions)]
pub(crate) struct MessageKeyBytes(pub(crate) Vec<u8>);

impl FromStr for MessageKeyBytes {
    type Err = DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BASE_64.decode(s).map(Self)
    }
}

impl AsRef<[u8]> for MessageKeyBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl MessageKeyBytes {
    pub fn to_exposed(&self) -> SessionKeyExposed {
        SessionKeyExposed(BASE_64.encode(&self.0))
    }
}

/// A message session key that was used to encrypt a message body.
/// The type internally stores the secret message key bytes and the
/// symmetric encryption algorithm that was used for encryption.
///
/// The secret part is wiped from memory on drop.
#[derive(Clone, Eq, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub struct MessageSessionKey {
    pub(crate) session_key_bytes: MessageKeyBytes,
    pub(crate) session_key_algorithm: SessionKeyAlgorithm,
}

impl MessageSessionKey {
    /// Import a message key from a session key used by an `OpenPGP` provider
    /// (See [`proton_crypto_account::proton_crypto`]).
    ///
    /// # Parameters
    ///
    /// * `session_key` - The session key to import.
    ///
    /// # Errors
    ///
    /// Returns a [`MessageError::InvalidSessionKey`] if the session key is invalid.
    /// This can happen if no session key algorithm can be retrieved or the algorithm is not
    /// compatible with the exported bytes.
    pub fn import_from_pgp_provider<Sk: SessionKey>(
        session_key: &Sk,
    ) -> Result<Self, MessageError> {
        let session_key_bytes = MessageKeyBytes(session_key.export().as_ref().to_owned());
        let session_key_algorithm = session_key
            .algorithm()
            .ok_or(MessageError::InvalidSessionKey)?;
        if !session_key_algorithm.is_compatible(session_key_bytes.as_ref()) {
            return Err(MessageError::InvalidSessionKey);
        }
        Ok(MessageSessionKey {
            session_key_bytes,
            session_key_algorithm,
        })
    }

    /// Exports the message key to a session key from a `OpenPGP` provider.
    /// (See [`proton_crypto_account::proton_crypto`]).
    ///
    /// # Parameters
    ///
    /// * `pgp_provider` - The `OpenPGP` provider to export to.
    ///
    /// # Errors
    ///
    /// A [`CryptoError`] if `OpenPGP` provider fails to accept the key.
    pub fn export_to_pgp_provider<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
    ) -> Result<Provider::SessionKey, CryptoError> {
        pgp_provider.session_key_import(self.session_key_bytes.as_ref(), self.session_key_algorithm)
    }

    /// Exposes the internal session key as a base64 encoded string, which is wiped from memory on drop.
    pub fn expose_secret(&self) -> SessionKeyExposed {
        self.session_key_bytes.to_exposed()
    }

    /// Returns the symmetric encryption algorithm the session key is associated with.
    pub fn algorithm(&self) -> SessionKeyAlgorithm {
        self.session_key_algorithm
    }
}
