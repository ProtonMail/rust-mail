use std::str::FromStr;

use base64::{DecodeError, Engine as _, prelude::BASE64_STANDARD as BASE_64};
use proton_crypto_account::proton_crypto::crypto::{
    AsPublicKeyRef, Encryptor, EncryptorSync, PGPProviderSync, SessionKey, SessionKeyAlgorithm,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::SessionKeyError;

crate::string_id! {
    /// A key packet represents a single encrypted inbox session key with an `OpenPGP` key.
    ///
    /// The key packet is represented as a base64 encoded string.
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
        BASE_64.decode(&self.0)
    }
}

crate::string_id! {
    /// Exposed secret inbox session key encoded as a base64 string.
    ///
    /// The message key is a symmetric `OpenPGP` session key used
    /// for symmetric encryption.
    /// Its memory is zeroed after drop. Note that this
    /// type should be treated as a sensitive secret.
    ///
    /// For example, the session key has to be exposed to the backend
    /// if an email package is sent to a recipient with cleartext only.
    SessionKeyExposed
}

impl SessionKeyExposed {
    /// Decodes the session key and exposes it.
    pub fn decode(&self) -> Result<Vec<u8>, DecodeError> {
        BASE_64.decode(&self.0)
    }
}

impl ZeroizeOnDrop for SessionKeyExposed {}

impl Drop for SessionKeyExposed {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// A secret symmetric message key that might be revealed to the backend.
///
/// It is wiped from memory on drop.
#[derive(Clone, Eq, PartialEq, Zeroize, ZeroizeOnDrop)]
#[allow(clippy::module_name_repetitions)]
pub(crate) struct SessionKeyBytes(pub(crate) Vec<u8>);

impl FromStr for SessionKeyBytes {
    type Err = DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BASE_64.decode(s).map(Self)
    }
}

impl AsRef<[u8]> for SessionKeyBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl SessionKeyBytes {
    pub fn to_exposed(&self) -> SessionKeyExposed {
        SessionKeyExposed(BASE_64.encode(&self.0))
    }
}

/// A inbox session key that was used to encrypt a message or attachment body.
/// This type internally stores the secret session key bytes and the
/// symmetric encryption algorithm that was used for encryption.
///
/// The secret part is wiped from memory on drop.
#[derive(Clone, Eq, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub struct InboxSessionKey {
    pub(crate) session_key_bytes: SessionKeyBytes,
    pub(crate) session_key_algorithm: SessionKeyAlgorithm,
}

impl InboxSessionKey {
    /// Imports an inbox session key from a session key used by an `OpenPGP` provider
    /// (See [`proton_crypto_account::proton_crypto`]).
    pub fn import_from_pgp_provider<Sk: SessionKey>(
        session_key: &Sk,
    ) -> Result<Self, SessionKeyError> {
        let session_key_bytes: SessionKeyBytes =
            SessionKeyBytes(session_key.export().as_ref().to_owned());
        let session_key_algorithm = session_key.algorithm();
        if session_key_algorithm == SessionKeyAlgorithm::Unknown {
            // Can happen for session keys extracted from v6 PKESK packets.
            // `import_from_pgp_provider_with_algorithm` should be called in this case.
            return Err(SessionKeyError::InvalidSessionKey(
                "no associated algorithm found".to_owned(),
            ));
        }
        Ok(InboxSessionKey {
            session_key_bytes,
            session_key_algorithm,
        })
    }

    /// Imports an inbox session key from a session key used by an `OpenPGP` provider
    /// (See [`proton_crypto_account::proton_crypto`]).
    ///
    /// # Errors
    ///
    /// Returns a [`SessionKeyError::InvalidSessionKey`] if the session key is invalid.
    /// This can happen if the provided algorithm does not match the extracted session key algorithm
    /// if any is extracted.
    pub fn import_from_pgp_provider_with_algorithm<Sk: SessionKey>(
        session_key: &Sk,
        algorithm: SessionKeyAlgorithm,
    ) -> Result<Self, SessionKeyError> {
        let session_key_bytes: SessionKeyBytes =
            SessionKeyBytes(session_key.export().as_ref().to_owned());
        let session_key_algorithm = session_key.algorithm();
        match session_key_algorithm {
            SessionKeyAlgorithm::Aes128 | SessionKeyAlgorithm::Aes256 => {
                if session_key_algorithm != algorithm {
                    return Err(SessionKeyError::InvalidSessionKey(
                        "algorithms do not match".to_owned(),
                    ));
                }
            }
            SessionKeyAlgorithm::Unknown => (),
        }

        Ok(InboxSessionKey {
            session_key_bytes,
            session_key_algorithm: algorithm,
        })
    }

    /// Exports the inbox session key to a session key for an `OpenPGP` provider
    /// (See [`proton_crypto_account::proton_crypto`]).
    pub fn export_to_pgp_provider<P>(&self, pgp: &P) -> Result<P::SessionKey, SessionKeyError>
    where
        P: PGPProviderSync,
    {
        pgp.session_key_import(self.session_key_bytes.as_ref(), self.session_key_algorithm)
            .map_err(SessionKeyError::Import)
    }

    /// Exposes the internal session key as a base64 encoded string, which is wiped from memory on drop.
    #[must_use]
    pub fn expose_secret(&self) -> SessionKeyExposed {
        self.session_key_bytes.to_exposed()
    }

    /// Returns the symmetric encryption algorithm the session key is associated with.
    #[must_use]
    pub fn algorithm(&self) -> SessionKeyAlgorithm {
        self.session_key_algorithm
    }

    /// Creates a key packet for the provided recipient public key.
    ///
    /// Encrypts the internal symmetric session key with the provided public key
    /// using `OpenPGP`. The output is an `OpenPGP` PKESK packet (referred to as a key packet in the Proton context).
    pub fn encrypt_to_recipient<P>(
        &self,
        pgp: &P,
        recipient_key: &impl AsPublicKeyRef<P::PublicKey>,
    ) -> Result<KeyPacket, SessionKeyError>
    where
        P: PGPProviderSync,
    {
        let session_key = self.export_to_pgp_provider(pgp)?;

        pgp.new_encryptor()
            .with_encryption_key(recipient_key.as_public_key())
            .encrypt_session_key(&session_key)
            .map(|key_packet| KeyPacket::new_from_bytes(key_packet.as_ref()))
            .map_err(SessionKeyError::KeyPacketEncryption)
    }

    /// Creates a key packet for the provided password.
    ///
    /// Encrypts the internal symmetric session key with the password
    /// using `OpenPGP`. The output is an `OpenPGP` SKESK packet (referred to as a key packet in the Proton context).
    pub fn encrypt_to_password<P>(
        &self,
        pgp: &P,
        passphrase: &str,
    ) -> Result<KeyPacket, SessionKeyError>
    where
        P: PGPProviderSync,
    {
        let session_key = self.export_to_pgp_provider(pgp)?;

        pgp.new_encryptor()
            .with_passphrase(passphrase)
            .encrypt_session_key(&session_key)
            .map(|key_packet| KeyPacket::new_from_bytes(key_packet.as_ref()))
            .map_err(SessionKeyError::KeyPacketEncryption)
    }

    /// Creates a key packet for each provided recipient public key.
    ///
    /// Encrypts the internal symmetric session key with the provided public keys
    /// using `OpenPGP`. The output is an `OpenPGP` PKESK packet (referred to as a key packet in the Proton context).
    /// The key packets are returned in the order of the provided recipient public keys.
    pub fn encrypt_to_recipients<P>(
        &self,
        pgp: &P,
        recipient_keys: &[impl AsPublicKeyRef<P::PublicKey>],
    ) -> Result<Vec<KeyPacket>, SessionKeyError>
    where
        P: PGPProviderSync,
    {
        let session_key = self.export_to_pgp_provider(pgp)?;

        // Encrypt the session key to each recipient key.
        let mut key_packets = Vec::with_capacity(recipient_keys.len());

        for encryption_key in recipient_keys {
            let key_packet = pgp
                .new_encryptor()
                .with_encryption_key(encryption_key.as_public_key())
                .encrypt_session_key(&session_key)
                .map(|key_packet| KeyPacket::new_from_bytes(key_packet.as_ref()))
                .map_err(SessionKeyError::KeyPacketEncryption)?;

            key_packets.push(key_packet);
        }

        Ok(key_packets)
    }
}
