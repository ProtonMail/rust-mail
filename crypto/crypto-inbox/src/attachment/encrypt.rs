use std::{
    io::{self, Write},
    string::FromUtf8Error,
};

use proton_crypto_account::proton_crypto::{
    crypto::{
        ArmorerSync, AsPublicKeyRef, DataEncoding, DetachedSignatureVariant, Encryptor,
        EncryptorDetachedSignatureWriter, EncryptorSync, EncryptorWriter, PGPProviderSync,
    },
    CryptoError,
};

use super::{AttachmentDecryption, AttachmentEncryptedSignature, AttachmentSignature, KeyPackets};

/// Type for encryption metadata belonging to a specific encrypted attachment.
///
/// The type can contain an encrypted and unencrypted signature (legacy).
/// For legacy during the transition period, both have to be provided.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct EncryptedAttachmentMetadata {
    /// Optional attachment signature.
    pub signature: Option<AttachmentSignature>,
    /// Optional encrypted attachment signature.
    pub encrypted_signature: Option<AttachmentEncryptedSignature>,
    /// The encrypted session key for each recipient key.
    pub key_packets: KeyPackets,
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
}

/// Encrypts an attachment to each key in `encryption_keys` and produces a signature for each key in `signing_keys`.
///
/// The output [`EncryptedAttachment`] consists of the encrypted attachment and the [`EncryptedAttachmentMetadata`]
/// containing the key packets, signatures, and encrypted signature.
/// If no signing keys are provided, i.e., a zero length slice, no signatures are produced.
///
/// # Parameters
///
/// * `pgp_provider`    - The pgp provider instance from `proton_crypto`.
/// * `encryption_keys` - The encryption keys of the recipients to encrypt the attachment to.
/// * `signing_keys`    - The signing keys of the user that the attachment is signed with.
/// * `attachment_data` - The attachment data to encrypt.
pub fn encrypt<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
    encryption_keys: &[impl AsPublicKeyRef<Provider::PublicKey>],
    signing_keys: &[impl AsRef<Provider::PrivateKey>],
    attachment_data: impl AsRef<[u8]>,
) -> Result<EncryptedAttachment, AttachmentEncryptionError> {
    if encryption_keys.is_empty() {
        return Err(AttachmentEncryptionError::NoKeys);
    }
    if signing_keys.is_empty() {
        // Handle the case where no signatures should be produced.
        return encrypt_attachment_without_signing(pgp_provider, encryption_keys, attachment_data);
    }
    // Generate a fresh PGP session key for encrypting the data and the signature.
    let session_key = pgp_provider
        .new_encryptor()
        .with_encryption_key_refs(encryption_keys)
        .generate_session_key()
        .map_err(AttachmentEncryptionError::SessionKeyGeneration)?;

    // Encrypt the session key with all the provide encryption_keys.
    // The encrypted session key packets are called key packets for brevity.
    let key_packets = pgp_provider
        .new_encryptor()
        .with_encryption_key_refs(encryption_keys)
        .encrypt_session_key(&session_key)
        .map_err(AttachmentEncryptionError::SessionKeyEncryption)?;

    // Encrypt the data with the session key and produce the detached signature.
    let (data, detached_signature_bytes, detached_signature) =
        encrypt_attachment_and_sign_detached(
            pgp_provider,
            &session_key,
            signing_keys,
            attachment_data,
        )?;

    // Encrypt the detached signature with the session key and attach the key packets.
    let encrypted_detached_signature = encrypt_detached_signature(
        pgp_provider,
        &key_packets,
        &session_key,
        &detached_signature_bytes,
    )?;

    let metadata = EncryptedAttachmentMetadata {
        signature: Some(detached_signature),
        encrypted_signature: Some(encrypted_detached_signature),
        key_packets: KeyPackets::new_from_bytes(&key_packets),
    };
    Ok(EncryptedAttachment { metadata, data })
}

/// Encrypts an attachment to each key in `encryption_keys` and produces a signature for each key in `signing_keys`.
///
/// The output [`SigncryptedAttachmentWriter`] is a writer where the attachment can be written to
/// for encryption.
/// Both `encryption_keys` and `signing_keys` must contain at least one key else an error is thrown.
///
/// # Parameters
///
/// * `pgp_provider`    - The pgp provider instance from `proton_crypto`.
/// * `encryption_keys` - The encryption keys of the recipients to encrypt the attachment to.
/// * `signing_keys`    - The signing keys of the user that the attachment is signed with.
/// * `attachment_data` - A writer where the encrypted attachment is written to.
pub fn encrypt_and_sign_to_writer<'a, Provider: PGPProviderSync, W: Write + 'a>(
    pgp_provider: &'a Provider,
    encryption_keys: &'a [impl AsPublicKeyRef<Provider::PublicKey>],
    signing_keys: &'a [impl AsRef<Provider::PrivateKey>],
    attachment_data: W,
) -> Result<
    SigncryptedAttachmentWriter<'a, W, Provider, Provider::Encryptor<'a>>,
    AttachmentEncryptionError,
> {
    if encryption_keys.is_empty() {
        return Err(AttachmentEncryptionError::NoKeys);
    }

    if signing_keys.is_empty() {
        return Err(AttachmentEncryptionError::NoSigningKeys);
    }

    // Generate a fresh PGP session key for encrypting the data and the signature.
    let session_key = pgp_provider
        .new_encryptor()
        .with_encryption_key_refs(encryption_keys)
        .generate_session_key()
        .map_err(AttachmentEncryptionError::SessionKeyGeneration)?;

    // Encrypt the session key with all the provided encryption_keys.
    // The encrypted session key packets are called key packets for brevity.
    let key_packets = pgp_provider
        .new_encryptor()
        .with_encryption_key_refs(encryption_keys)
        .encrypt_session_key(&session_key)
        .map_err(AttachmentEncryptionError::SessionKeyEncryption)?;

    let attachment_writer = pgp_provider
        .new_encryptor()
        .with_session_key(session_key.clone())
        .with_signing_key_refs(signing_keys)
        .encrypt_stream_with_detached_signature(
            attachment_data,
            DetachedSignatureVariant::Plaintext,
            DataEncoding::Bytes,
        )
        .map(|writer| SigncryptedAttachmentWriter {
            pgp_provider,
            writer,
            session_key,
            key_packets,
        })
        .map_err(AttachmentEncryptionError::Encryption)?;
    Ok(attachment_writer)
}

/// Encrypts an attachment to each key in `encryption_keys` but does not produce any signatures.
///
/// The output [`EncryptedAttachmentWriter`] is a writer where the attachment can be written to
/// for encryption.
/// `encryption_keys`must contain at least one key else an error is thrown.
///
/// # Warning
///
/// Use [`encrypt_and_sign_to_writer`] whenever possible to create signatures over the attachment.
///
/// # Parameters
///
/// * `pgp_provider`    - The pgp provider instance from `proton_crypto`.
/// * `encryption_keys` - The encryption keys of the recipients to encrypt the attachment to.
/// * `attachment_data` - A writer where the encrypted attachment is written to.
pub fn encrypt_to_writer<'a, Provider: PGPProviderSync, W: Write + 'a>(
    pgp_provider: &'a Provider,
    encryption_keys: &'a [impl AsPublicKeyRef<Provider::PublicKey>],
    attachment_data: W,
) -> Result<EncryptedAttachmentWriter<'a, W, Provider::Encryptor<'a>>, AttachmentEncryptionError> {
    if encryption_keys.is_empty() {
        return Err(AttachmentEncryptionError::NoKeys);
    }

    let attachment_writer = pgp_provider
        .new_encryptor()
        .with_encryption_key_refs(encryption_keys)
        .encrypt_stream_split(attachment_data)
        .map(|(key_packets, writer)| {
            EncryptedAttachmentWriter(KeyPackets::new_from_bytes(&key_packets), writer)
        })
        .map_err(AttachmentEncryptionError::Encryption)?;
    Ok(attachment_writer)
}

fn encrypt_attachment_and_sign_detached<T: PGPProviderSync>(
    pgp_provider: &T,
    session_key: &T::SessionKey,
    signing_keys: &[impl AsRef<T::PrivateKey>],
    attachment_data: impl AsRef<[u8]>,
) -> Result<(Vec<u8>, Vec<u8>, AttachmentSignature), AttachmentEncryptionError> {
    let mut data = Vec::with_capacity(attachment_data.as_ref().len());
    let detached_signature_bytes = {
        // Encrypt the data and produce a detached signature.
        let mut pt_writer = pgp_provider
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
    // The detached signature is a byte blob, but the output has to be pgp armored.
    let detached_signature = armor_detached_signature(pgp_provider, &detached_signature_bytes)?;
    Ok((data, detached_signature_bytes, detached_signature))
}

// Armor the detached signature and convert it to an owned String.
fn armor_detached_signature<T: PGPProviderSync>(
    pgp_provider: &T,
    detached_signature_bytes: &[u8],
) -> Result<AttachmentSignature, AttachmentEncryptionError> {
    let detached_signature_armored = pgp_provider
        .armorer()
        .armor_signature(detached_signature_bytes)
        .map_err(AttachmentEncryptionError::Encryption)?;
    let signature = String::from_utf8(detached_signature_armored).map(AttachmentSignature)?;
    Ok(signature)
}

// Encrypt the detached signature with the session key and attach the key packets.
fn encrypt_detached_signature<T: PGPProviderSync>(
    pgp_provider: &T,
    key_packets: &[u8],
    session_key: &T::SessionKey,
    detached_signature_bytes: &[u8],
) -> Result<AttachmentEncryptedSignature, AttachmentEncryptionError> {
    let mut encrypted_signature_bytes =
        Vec::with_capacity(key_packets.len() + detached_signature_bytes.len());
    // The encrypted pgp message consists of `key_packets || encrypted_data`
    encrypted_signature_bytes.extend_from_slice(key_packets);
    // Write encrypted_data.
    let mut pt_signature_writer = pgp_provider
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

    // PGP armor the encrypted detached signature.
    let encrypted_detached_signature_armored = pgp_provider
        .armorer()
        .armor_message(&encrypted_signature_bytes)
        .map_err(AttachmentEncryptionError::SignatureEncryption)?;
    let attachment_encrypted_signature = String::from_utf8(encrypted_detached_signature_armored)
        .map(AttachmentEncryptedSignature)?;
    Ok(attachment_encrypted_signature)
}

fn encrypt_attachment_without_signing<T: PGPProviderSync>(
    pgp_provider: &T,
    encryption_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    attachment_data: impl AsRef<[u8]>,
) -> Result<EncryptedAttachment, AttachmentEncryptionError> {
    let mut data = Vec::with_capacity(attachment_data.as_ref().len());
    let key_packets = {
        let (key_packets, mut pt_writer) = pgp_provider
            .new_encryptor()
            .with_encryption_key_refs(encryption_keys)
            .encrypt_stream_split(&mut data)
            .map_err(AttachmentEncryptionError::Encryption)?;
        pt_writer
            .write_all(attachment_data.as_ref())
            .map_err(|err| AttachmentEncryptionError::Encryption(err.into()))?;
        key_packets
    };
    let metadata = EncryptedAttachmentMetadata {
        signature: None,
        encrypted_signature: None,
        key_packets: KeyPackets::new_from_bytes(&key_packets),
    };
    Ok(EncryptedAttachment { metadata, data })
}

impl AttachmentDecryption for EncryptedAttachmentMetadata {
    fn attachment_key_packets(&self) -> &KeyPackets {
        &self.key_packets
    }

    fn attachment_signature(&self) -> &Option<AttachmentSignature> {
        &self.signature
    }

    fn attachment_encrypted_signature(&self) -> &Option<AttachmentEncryptedSignature> {
        &self.encrypted_signature
    }
}

/// Attachment writer for encrypting and signing data.
#[derive(Debug)]
pub struct SigncryptedAttachmentWriter<'a, W, Provider, ProvEncryptor>
where
    W: Write + 'a,
    Provider: PGPProviderSync,
    ProvEncryptor: Encryptor<'a>,
{
    pgp_provider: &'a Provider,
    writer: ProvEncryptor::EncryptorDetachedSignatureWriter<'a, W>,
    session_key: Provider::SessionKey,
    key_packets: Vec<u8>,
}

impl<'a, W, Provider, ProvEncryptor> Write
    for SigncryptedAttachmentWriter<'a, W, Provider, ProvEncryptor>
where
    W: Write + 'a,
    Provider: PGPProviderSync,
    ProvEncryptor: Encryptor<'a>,
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
        let detached_signature =
            armor_detached_signature(self.pgp_provider, &detached_signature_bytes)?;
        let encrypted_detached_signature = encrypt_detached_signature(
            self.pgp_provider,
            &self.key_packets,
            &self.session_key,
            &detached_signature_bytes,
        )?;
        let metadata = EncryptedAttachmentMetadata {
            signature: Some(detached_signature),
            encrypted_signature: Some(encrypted_detached_signature),
            key_packets: KeyPackets::new_from_bytes(&self.key_packets),
        };
        Ok(metadata)
    }
}

/// Attachment writer for encryption only without signing.
#[derive(Debug)]
pub struct EncryptedAttachmentWriter<'a, W: Write + 'a, ProvEncryptor: Encryptor<'a>>(
    KeyPackets,
    ProvEncryptor::EncryptorWriter<'a, W>,
);

impl<'a, W: Write + 'a, ProvEncryptor: Encryptor<'a>> Write
    for EncryptedAttachmentWriter<'a, W, ProvEncryptor>
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.1.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.1.flush()
    }
}

impl<'a, W: Write + 'a, ProvEncryptor: Encryptor<'a>>
    EncryptedAttachmentWriter<'a, W, ProvEncryptor>
{
    /// Finalizes the encryption and returns the [`EncryptedAttachmentMetadata`].
    ///
    /// Must be called once all attachment data has been written to this writer.
    pub fn finalize(self) -> Result<EncryptedAttachmentMetadata, AttachmentEncryptionError> {
        self.1
            .finalize()
            .map_err(AttachmentEncryptionError::Encryption)?;
        let metadata = EncryptedAttachmentMetadata {
            signature: None,
            encrypted_signature: None,
            key_packets: self.0,
        };
        Ok(metadata)
    }
}
