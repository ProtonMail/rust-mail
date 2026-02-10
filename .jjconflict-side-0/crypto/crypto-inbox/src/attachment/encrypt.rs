use std::{
    io::{self, Write},
    string::FromUtf8Error,
};

use proton_crypto_account::{
    keys::PrimaryUnlockedAddressKey,
    proton_crypto::{
        CryptoError,
        crypto::{
            AsPublicKeyRef, DataEncoding, DetachedSignatureVariant, Encryptor,
            EncryptorDetachedSignatureWriter, EncryptorSync, EncryptorWriter, PGPProviderSync,
        },
    },
};

use crate::keys::{InboxSessionKey, KeyPacket, SessionKeyError};

use super::{ArmorEncodingError, BinaryAttachmentEncryptedSignature, BinaryAttachmentSignature};

/// Type for encryption metadata belonging to a specific encrypted attachment.
///
/// The type can contain an encrypted and unencrypted signature (legacy).
/// For legacy during the transition period, both have to be provided.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct EncryptedAttachmentMetadata {
    /// Optional attachment signature.
    pub signature: Option<BinaryAttachmentSignature>,
    /// Optional encrypted attachment signature.
    pub encrypted_signature: Option<BinaryAttachmentEncryptedSignature>,
    /// The encrypted session key for each recipient key.
    pub key_packets: Vec<u8>,
}

/// Represent an attachment that is encrypted.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct EncryptedAttachment {
    /// Encryption metadata needed for decryption and verification.
    pub metadata: EncryptedAttachmentMetadata,
    /// Encrypted attachment data.
    pub data: Vec<u8>,
}

/// Errors that can be returned in attachment encryption.
#[derive(Debug, thiserror::Error)]
pub enum AttachmentEncryptionError {
    #[error("Failed to encrypt and sign attachment: {0}")]
    Encryption(CryptoError),
    #[error("Failed to to convert to utf-8 string: {0}")]
    Encoding(#[from] FromUtf8Error),
    #[error("Failed to encrypt attached signature: {0}")]
    SignatureEncryption(CryptoError),
    #[error("No encryption keys provided")]
    NoKeys,
    #[error("No signing keys provided")]
    NoSigningKeys,
    #[error("Session key generation failed: {0}")]
    SessionKeyGeneration(CryptoError),
    #[error("Session key encryption failed: {0}")]
    SessionKeyEncryption(CryptoError),
    #[error("Invalid session key: {0}")]
    SessionKeyProblem(#[from] SessionKeyError),
    #[error("Encrypted signature armor: {0}")]
    Armor(#[from] ArmorEncodingError),
}

pub trait EncryptableAttachment {
    /// Returns the plaintext attachment data.
    fn attachment_data(&self) -> &[u8];

    /// Encrypts and signs an attachment with the primary address key.
    ///
    /// The output [`EncryptedAttachment`] consists of the encrypted attachment and the [`EncryptedAttachmentMetadata`]
    /// containing the key packets, signatures, and encrypted signature.
    fn attachment_encrypt_and_sign<P>(
        &self,
        pgp: &P,
        primary_address_key: &PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
    ) -> Result<EncryptedAttachment, AttachmentEncryptionError>
    where
        P: PGPProviderSync,
    {
        encrypt(pgp, primary_address_key, self.attachment_data())
    }
}

/// Encrypts and signs an attachment with the primary address key.
///
/// The output [`EncryptedAttachment`] consists of the encrypted attachment and the [`EncryptedAttachmentMetadata`]
/// containing the key packets, signatures, and encrypted signature.
pub fn encrypt<P>(
    pgp: &P,
    primary_address_key: &PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
    attachment_data: impl AsRef<[u8]>,
) -> Result<EncryptedAttachment, AttachmentEncryptionError>
where
    P: PGPProviderSync,
{
    encrypt_helper(
        pgp,
        &[primary_address_key.for_encryption()],
        primary_address_key.for_signing(),
        attachment_data,
    )
}

/// Encrypts an attachment to each key in `encryption_keys` and produces a signature for each key in `signing_keys`.
///
/// The output [`EncryptedAttachment`] consists of the encrypted attachment and the [`EncryptedAttachmentMetadata`]
/// containing the key packets, signatures, and encrypted signature.
/// If no signing keys are provided, i.e., a zero length slice, no signatures are produced.
fn encrypt_helper<P>(
    pgp: &P,
    encryption_keys: &[impl AsPublicKeyRef<P::PublicKey>],
    signing_keys: &[impl AsRef<P::PrivateKey>],
    attachment_data: impl AsRef<[u8]>,
) -> Result<EncryptedAttachment, AttachmentEncryptionError>
where
    P: PGPProviderSync,
{
    if encryption_keys.is_empty() {
        return Err(AttachmentEncryptionError::NoKeys);
    }

    // Generate a fresh PGP session key for encrypting the data and the signature.
    let session_key = pgp
        .new_encryptor()
        .with_encryption_key_refs(encryption_keys)
        .generate_session_key()
        .map_err(AttachmentEncryptionError::SessionKeyGeneration)?;

    // Encrypt the session key with all the provide encryption_keys.
    // The encrypted session key packets are called key packets for brevity.
    let key_packets = pgp
        .new_encryptor()
        .with_encryption_key_refs(encryption_keys)
        .encrypt_session_key(&session_key)
        .map_err(AttachmentEncryptionError::SessionKeyEncryption)?;

    // Encrypt the data with the session key and produce the detached signature.
    let (data, detached_signature_bytes) =
        encrypt_attachment_and_sign_detached(pgp, &session_key, signing_keys, attachment_data)?;

    // Encrypt the detached signature with the session key and attach the key packets.
    let encrypted_detached_signature =
        encrypt_detached_signature(pgp, &key_packets, &session_key, &detached_signature_bytes)?;

    let metadata = EncryptedAttachmentMetadata {
        signature: Some(detached_signature_bytes),
        encrypted_signature: Some(encrypted_detached_signature),
        key_packets,
    };

    Ok(EncryptedAttachment { metadata, data })
}

/// Creates an encryption writer [`SigncryptedAttachmentWriter`], where each write operation results
/// in writing encrypted data to the provided writer.
///
/// The key packets and signatures (i.e., attachment metadata) can be accessed with [`SigncryptedAttachmentWriter::finalize`]
/// once all data has been written.
pub fn encrypt_and_sign_to_writer<'a, P, W>(
    pgp: &'a P,
    primary_address_key: &'a PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
    attachment_data: W,
) -> Result<SigncryptedAttachmentWriter<'a, W, P, P::Encryptor<'a>>, AttachmentEncryptionError>
where
    P: PGPProviderSync,
    W: Write + 'a,
{
    encrypt_and_sign_to_writer_helper(
        pgp,
        primary_address_key.for_encryption(),
        primary_address_key.for_signing(),
        attachment_data,
    )
}

/// Streaming attachment encryption helper.
fn encrypt_and_sign_to_writer_helper<'a, P, W>(
    pgp: &'a P,
    encryption_key: &'a impl AsPublicKeyRef<P::PublicKey>,
    signing_keys: &'a [impl AsRef<P::PrivateKey>],
    attachment_data: W,
) -> Result<SigncryptedAttachmentWriter<'a, W, P, P::Encryptor<'a>>, AttachmentEncryptionError>
where
    P: PGPProviderSync,
    W: Write + 'a,
{
    if signing_keys.is_empty() {
        return Err(AttachmentEncryptionError::NoSigningKeys);
    }

    // Generate a fresh PGP session key for encrypting the data and the signature.
    let session_key = pgp
        .new_encryptor()
        .with_encryption_key(encryption_key.as_public_key())
        .generate_session_key()
        .map_err(AttachmentEncryptionError::SessionKeyGeneration)?;

    // Encrypt the session key with all the provided encryption_keys.
    // The encrypted session key packets are called key packets for brevity.
    let key_packets = pgp
        .new_encryptor()
        .with_encryption_key(encryption_key.as_public_key())
        .encrypt_session_key(&session_key)
        .map_err(AttachmentEncryptionError::SessionKeyEncryption)?;

    let attachment_writer = pgp
        .new_encryptor()
        .with_session_key(session_key.clone())
        .with_signing_key_refs(signing_keys)
        .encrypt_stream_with_detached_signature(
            attachment_data,
            DetachedSignatureVariant::Plaintext,
            DataEncoding::Bytes,
        )
        .map(|writer| SigncryptedAttachmentWriter {
            pgp,
            writer,
            session_key,
            key_packets,
        })
        .map_err(AttachmentEncryptionError::Encryption)?;

    Ok(attachment_writer)
}

fn encrypt_attachment_and_sign_detached<P>(
    pgp: &P,
    session_key: &P::SessionKey,
    signing_keys: &[impl AsRef<P::PrivateKey>],
    attachment_data: impl AsRef<[u8]>,
) -> Result<(Vec<u8>, BinaryAttachmentSignature), AttachmentEncryptionError>
where
    P: PGPProviderSync,
{
    let mut data = Vec::with_capacity(attachment_data.as_ref().len());

    let detached_signature_bytes = {
        // Encrypt the data and produce a detached signature.
        let mut pt_writer = pgp
            .new_encryptor()
            .with_session_key_ref(session_key)
            .with_signing_key_refs(signing_keys)
            .encrypt_stream_with_detached_signature(
                &mut data,
                DetachedSignatureVariant::Plaintext,
                DataEncoding::Bytes,
            )
            .map_err(AttachmentEncryptionError::Encryption)?;

        pt_writer
            .write_all(attachment_data.as_ref())
            .map_err(|err| AttachmentEncryptionError::Encryption(err.into()))?;

        pt_writer
            .finalize_with_detached_signature()
            .map_err(AttachmentEncryptionError::Encryption)?
    };

    Ok((data, BinaryAttachmentSignature(detached_signature_bytes)))
}

// Encrypt the detached signature with the session key and attach the key packets.
fn encrypt_detached_signature<P>(
    pgp: &P,
    key_packets: &[u8],
    session_key: &P::SessionKey,
    detached_signature_bytes: &[u8],
) -> Result<BinaryAttachmentEncryptedSignature, AttachmentEncryptionError>
where
    P: PGPProviderSync,
{
    let mut encrypted_signature_bytes =
        Vec::with_capacity(key_packets.len() + detached_signature_bytes.len());

    // The encrypted pgp message consists of `key_packets || encrypted_data`
    encrypted_signature_bytes.extend_from_slice(key_packets);

    // Write encrypted_data.
    let mut pt_signature_writer = pgp
        .new_encryptor()
        .with_session_key_ref(session_key)
        .encrypt_stream(&mut encrypted_signature_bytes, DataEncoding::Bytes)
        .map_err(AttachmentEncryptionError::SignatureEncryption)?;

    pt_signature_writer
        .write_all(detached_signature_bytes.as_ref())
        .map_err(|err| AttachmentEncryptionError::SignatureEncryption(err.into()))?;

    pt_signature_writer
        .finalize()
        .map_err(AttachmentEncryptionError::SignatureEncryption)?;

    Ok(BinaryAttachmentEncryptedSignature(
        encrypted_signature_bytes,
    ))
}

/// Attachment writer for encrypting and signing data.
#[derive(Debug)]
pub struct SigncryptedAttachmentWriter<'a, W, P, E>
where
    W: Write + 'a,
    P: PGPProviderSync,
    E: Encryptor<'a>,
{
    pgp: &'a P,
    writer: E::EncryptorDetachedSignatureWriter<'a, W>,
    session_key: P::SessionKey,
    key_packets: Vec<u8>,
}

impl<'a, W, P, E> Write for SigncryptedAttachmentWriter<'a, W, P, E>
where
    W: Write + 'a,
    P: PGPProviderSync,
    E: Encryptor<'a>,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl<'a, W, Provider, ProvEncryptor> SigncryptedAttachmentWriter<'a, W, Provider, ProvEncryptor>
where
    W: Write + 'a,
    Provider: PGPProviderSync,
    ProvEncryptor: Encryptor<'a>,
{
    /// Finalizes the encryption and returns the `EncryptedAttachmentMetadata`.
    ///
    /// Must be called once all attachment data has been written to this writer.
    pub fn finalize(self) -> Result<EncryptedAttachmentMetadata, AttachmentEncryptionError> {
        let detached_signature_bytes = self
            .writer
            .finalize_with_detached_signature()
            .map_err(AttachmentEncryptionError::Encryption)?;

        let encrypted_detached_signature = encrypt_detached_signature(
            self.pgp,
            &self.key_packets,
            &self.session_key,
            &detached_signature_bytes,
        )?;

        let metadata = EncryptedAttachmentMetadata {
            signature: Some(BinaryAttachmentSignature(detached_signature_bytes)),
            encrypted_signature: Some(encrypted_detached_signature),
            key_packets: self.key_packets,
        };

        Ok(metadata)
    }
}

/// Represents decrypted attachment information that can be re-encrypted for new recipients.
pub struct ExtractedAttachmentInfo {
    /// The decrypted session key used to encrypt the attachment.
    ///
    /// [`InboxSessionKey`] provides methods to re-encrypt the session key for new recipients.
    pub session_key: InboxSessionKey,
    /// Internal attachment detached signature bytes.
    pub(crate) detached_signature_bytes: Option<BinaryAttachmentSignature>,
}

impl ExtractedAttachmentInfo {
    /// Creates a key packet for the provided recipient public key.
    ///
    /// Encrypts the internal symmetric session key with the provided public key
    /// using `OpenPGP`. The output is an `OpenPGP` PKESK packet (referred to as a key packet in the Proton context).
    pub fn encrypt_session_key_to_recipient<P>(
        &self,
        pgp: &P,
        recipient_key: &impl AsPublicKeyRef<P::PublicKey>,
    ) -> Result<KeyPacket, SessionKeyError>
    where
        P: PGPProviderSync,
    {
        self.session_key.encrypt_to_recipient(pgp, recipient_key)
    }

    /// Creates a key packet for the provided passphrase.
    ///
    /// Encrypts the internal symmetric session key with the provided passphrase
    /// using `OpenPGP`. The output is an `OpenPGP` SKESK packet (referred to as a key packet in the Proton context).
    pub fn encrypt_session_key_to_password<P>(
        &self,
        pgp: &P,
        passphrase: &str,
    ) -> Result<KeyPacket, SessionKeyError>
    where
        P: PGPProviderSync,
    {
        self.session_key.encrypt_to_password(pgp, passphrase)
    }

    /// Encrypts the internal signature towards a new recipient if present.
    pub fn encrypt_signature_to_recipient<P>(
        &self,
        pgp: &P,
        recipient: &P::PublicKey,
    ) -> Result<Option<BinaryAttachmentEncryptedSignature>, AttachmentEncryptionError>
    where
        P: PGPProviderSync,
    {
        let Some(signature_bytes) = &self.detached_signature_bytes else {
            return Ok(None);
        };

        let encrypted_signature = pgp
            .new_encryptor()
            .with_encryption_key(recipient)
            .encrypt_raw(&**signature_bytes, DataEncoding::Bytes)
            .map(BinaryAttachmentEncryptedSignature)
            .map_err(AttachmentEncryptionError::SignatureEncryption)?;

        Ok(Some(encrypted_signature))
    }

    /// Returns the internal signature as an binary PGP Signature without encrypting it.
    #[must_use]
    pub fn signature<Provider: PGPProviderSync>(&self) -> Option<&BinaryAttachmentSignature> {
        self.detached_signature_bytes.as_ref()
    }
}
