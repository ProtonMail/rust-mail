use crate::attachments::LocalAttachmentId;
use crate::{DBResult, LocalAttachmentMetadata, MailSqliteConnectionImpl};
use proton_api_mail::domain::{AddressId, AttachmentMetadata};
use proton_sqlite3::rusqlite::Row;

impl<'c> MailSqliteConnectionImpl<'c> {
    pub(crate) fn create_attachment_ref_statement(
        &mut self,
    ) -> DBResult<CreateAttachmentRefStatement<'_>> {
        // We want to create a new entry if the attachment has not been reference yet and
        // ignore it in case it's already there.
        self.0.prepare("INSERT OR IGNORE INTO attachments (rid, name, size, mime_type, disposition,address_id) VALUES \
(?,?,?,?,?,?) ON CONFLICT (rid) DO NOTHING RETURNING id").map(CreateAttachmentRefStatement)
    }
}

/// Shared statement that can be used by either conversations or messages to initialize the
/// attachments table with the basic information present in the metadata.
pub(crate) struct CreateAttachmentRefStatement<'a>(proton_sqlite3::rusqlite::Statement<'a>);

impl<'a> CreateAttachmentRefStatement<'a> {
    pub(crate) fn insert(
        &mut self,
        address_id: Option<&AddressId>,
        metadata: &AttachmentMetadata,
    ) -> DBResult<LocalAttachmentId> {
        self.0.query_row(
            (
                &metadata.id,
                &metadata.name,
                metadata.size,
                &metadata.mime_type,
                metadata.disposition,
                address_id,
            ),
            |r| r.get(0),
        )
    }
}

pub(crate) struct LocalAttachmentMetadataSelector {}

impl LocalAttachmentMetadataSelector {
    pub(crate) fn query() -> &'static str {
        "SELECT att.id, att.rid, att.name, att.size, att.mime_type, att.disposition FROM attachments AS att"
    }

    pub(crate) fn from_row(r: &Row) -> DBResult<LocalAttachmentMetadata> {
        Ok(LocalAttachmentMetadata {
            id: r.get(0)?,
            rid: r.get(1)?,
            name: r.get(2)?,
            size: r.get(3)?,
            mime_type: r.get(4)?,
            disposition: r.get(5)?,
        })
    }
}
