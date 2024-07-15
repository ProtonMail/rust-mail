//! Implements cryptographic helper functions to encrypt and decrypt attachments.

#[allow(clippy::module_name_repetitions)]
mod decrypt;
pub use decrypt::*;

#[allow(clippy::module_name_repetitions)]
mod encrypt;
pub use encrypt::*;

use base64::{prelude::BASE64_STANDARD as BASE_64, Engine as _};

use crate::{keys::KeyPacket, string_id};

string_id! {
    /// Encrypted session keys of an attachment.
    KeyPackets
}

impl KeyPackets {
    pub fn from_vec(value: Vec<KeyPacket>) -> Self {
        Self(value.into_iter().map(|a| a.0).collect::<String>())
    }

    pub(crate) fn new_from_bytes(key_packets: &[u8]) -> Self {
        KeyPackets(BASE_64.encode(key_packets))
    }

    pub fn decode(&self) -> Result<Vec<u8>, base64::DecodeError> {
        BASE_64.decode(&self.0).map_err(Into::into)
    }
}

string_id! {
    /// Detached signature over the attachment.
    AttachmentSignature
}

impl AsRef<[u8]> for AttachmentSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

string_id! {
    /// Encrypted detached signature over the attachment.
    AttachmentEncryptedSignature
}

impl AsRef<[u8]> for AttachmentEncryptedSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}
