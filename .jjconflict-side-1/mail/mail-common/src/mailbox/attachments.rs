use crate::datatypes::AttachmentMetadata;
use std::path::PathBuf;

/// A decrypted attachment returned by [`Mailbox::get_attachment`].
#[derive(Debug)]
#[cfg_attr(feature = "test-utils", derive(Eq, PartialEq))]
pub struct DecryptedAttachment {
    /// Metadata of the decrypted attachment.
    pub attachment_metadata: AttachmentMetadata,
    /// Content buffer of the attachment
    // TODO: it's ok on mobile to have decrypted attachments in file system. However it's not the
    //       case for desktop. So add an alternative code (behind a feature) later to handle
    //       attachment differently:
    //         * Cache crypted data
    //         * Decrypt
    //         * Add an alternative to this field like `pub content: Vec<u8>`
    pub data_path: PathBuf,
    // /// The result of the signature verification.
    // pub verification_result: VerificationResult,
}
