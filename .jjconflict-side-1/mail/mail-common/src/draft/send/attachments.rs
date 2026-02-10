use super::MailType;
use crate::{draft::PackageError, models::Attachment};
use proton_mail_api::services::proton::{common::AttachmentId, prelude as api};
use std::collections::HashMap;
use tracing::error;

/// Builder for [`api::PackageAttachmentEntries`]; see that enum for details.
#[derive(Clone, Debug, PartialEq)]
pub enum PackageAttachmentEntries<T> {
    Draft(HashMap<AttachmentId, T>),
    Direct(Vec<T>),
}

impl<T> PackageAttachmentEntries<T> {
    pub fn new(ty: MailType) -> Self {
        match ty {
            MailType::Draft => Self::Draft(HashMap::new()),
            MailType::Direct => Self::Direct(Vec::new()),
        }
    }

    pub fn insert(&mut self, att: &Attachment, entry: T) -> Result<(), PackageError> {
        if att.local_id.is_none() {
            return Err(PackageError::AttachmentHasNoLocalId);
        }

        match (self, att.remote_id()) {
            // Sending a draft requires for attachment to be already uploaded,
            // because we need to know remote attachment id in order to build a
            // map from attachment id onto its key.
            //
            // (or, you know, whatever is this `entry` the caller wants to put
            // here, but in practice that's either attachment key or signature.)
            (Self::Draft(_), None) => {
                error!(
                    local_id = ?att.local_id,
                    "Found an attachment without remote id",
                );

                Err(PackageError::AttachmentHasNoRemoteId)
            }

            (Self::Draft(entries), Some(id)) => {
                entries.insert(id, entry);
                Ok(())
            }

            (Self::Direct(entries), None) => {
                entries.push(entry);
                Ok(())
            }

            // Conversely, sending a direct mail requires for attachment *not*
            // to be uploaded yet.
            //
            // That's because, as compared to drafts, sending a direct mail
            // causes both the mail to be dispatched and the attachments to be
            // saved - and both actions are carried out by the backend, both at
            // once (at least "at once" from our point of view, of course).
            //
            // After the mail is sent, we get a response containing remote ids
            // of those now-sent attachments -- so if an attachment we want to
            // send _already_ has a remote id, something must've gone wrong:
            //
            // - should we now create a new local attachment?
            // - should we overwrite this attachment's remote id later?
            // - should we detonate user's device?
            //
            // Fortunately, the answer is simple: let's just make a late-check
            // and if the attachment already has a remote id, let's bail out -
            // caller's fault!
            (Self::Direct(_), Some(remote_id)) => {
                error!(
                    local_id = ?att.local_id,
                    ?remote_id,
                    "Found an attachment with remote id",
                );

                Err(PackageError::AttachmentAlreadyHasRemoteId)
            }
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Draft(entries) => entries.is_empty(),
            Self::Direct(entries) => entries.is_empty(),
        }
    }
}

impl<T> From<PackageAttachmentEntries<T>> for api::PackageAttachmentEntries<T> {
    fn from(this: PackageAttachmentEntries<T>) -> Self {
        match this {
            PackageAttachmentEntries::Draft(entries) => Self::Draft(
                entries
                    .into_iter()
                    .map(|(id, entry)| (id.to_string(), entry))
                    .collect(),
            ),

            PackageAttachmentEntries::Direct(entries) => Self::Direct(entries),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datatypes::LocalAttachmentId;
    use crate::models::AttachmentType;

    fn att(local_id: Option<u64>, remote_id: Option<&str>) -> Attachment {
        let local_id = local_id.map(LocalAttachmentId::from);
        let attachment_type = AttachmentType::Remote(remote_id.map(AttachmentId::from));

        Attachment {
            local_id,
            attachment_type,
            ..Attachment::default()
        }
    }

    #[track_caller]
    fn assert_eq(lhs: Result<(), PackageError>, rhs: Result<(), PackageError>) {
        assert_eq!(
            lhs.map_err(|err| err.to_string()),
            rhs.map_err(|err| err.to_string()),
        );
    }

    #[test]
    fn draft() {
        let mut target = PackageAttachmentEntries::new(MailType::Draft);

        assert_eq(
            Ok(()),
            target.insert(&att(Some(1234), Some("d5nBB4MV")), "some payload"),
        );
        assert_eq(
            Err(PackageError::AttachmentHasNoLocalId),
            target.insert(&att(None, Some("d5nBB4MV")), "some payload"),
        );
        assert_eq(
            Err(PackageError::AttachmentHasNoRemoteId),
            target.insert(&att(Some(1234), None), "some payload"),
        );
    }

    #[test]
    fn direct() {
        let mut target = PackageAttachmentEntries::new(MailType::Direct);

        assert_eq(
            Ok(()),
            target.insert(&att(Some(1234), None), "some payload"),
        );
        assert_eq(
            Err(PackageError::AttachmentHasNoLocalId),
            target.insert(&att(None, None), "some payload"),
        );
        assert_eq(
            Err(PackageError::AttachmentAlreadyHasRemoteId),
            target.insert(&att(Some(1234), Some("d5nBB4MV")), "some payload"),
        );
    }
}
