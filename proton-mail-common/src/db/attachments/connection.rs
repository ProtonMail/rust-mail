use crate::db::attachments::LocalAttachmentId;
use crate::db::json::{deserialize_optional_json_from_row, JsonWriteBuffer};
use crate::db::{
    DBResult, LocalAttachment, LocalAttachmentMetadata, LocalConversationId, LocalMessageId,
    MailSqliteConnectionImpl,
};
use indoc::indoc;
use proton_api_mail::domain::{Attachment, AttachmentId, AttachmentMetadata, MessageAttachment};
use proton_api_mail::proton_api_core::domain::AddressId;
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature, AttachmentSignature, KeyPackets,
};
use proton_sqlite3::rusqlite::{OptionalExtension, Row};
use proton_sqlite3::utils::{mapped_rows_to_vec, RowIndexAllocator};
use proton_sqlite3::{bind_list_indexed, bind_list_indexed_recursive};

impl<'c> MailSqliteConnectionImpl<'c> {
    pub(crate) fn create_message_attachment_ref_statement(
        &mut self,
    ) -> DBResult<CreateMessageAttachmentRefStatement<'c>> {
        // We want to create a new entry if the attachment has not been reference yet and
        // ignore it in case it's already there.
        self.0
            .prepare(
                r"
INSERT INTO attachments (
    rid,
    name,
    size,
    mime_type,
    disposition,
    address_id,
    message_id
) VALUES (?,?,?,?,?,?,?)
ON CONFLICT (rid) DO UPDATE SET
    id=id,
    message_id=excluded.message_id
RETURNING id",
            )
            .map(CreateMessageAttachmentRefStatement)
    }

    pub(crate) fn create_conversation_attachment_ref_statement(
        &mut self,
    ) -> DBResult<CreateConversationAttachmentRefStatement<'c>> {
        // We want to create a new entry if the attachment has not been reference yet and
        // ignore it in case it's already there.
        self.0
            .prepare(
                r"
INSERT INTO attachments (
    rid,
    name,
    size,
    mime_type,
    disposition,
    address_id,
    conversation_id
) VALUES (?,?,?,?,?,?,?)
ON CONFLICT (rid) DO UPDATE SET
    id=id,
    conversation_id=excluded.conversation_id
RETURNING id",
            )
            .map(CreateConversationAttachmentRefStatement)
    }

    /// Create or update local attachment from the full attachment metadata.
    ///
    /// # Errors
    /// Returns errors if the query fails.
    pub fn create_or_update_attachment(
        &mut self,
        attachment: &Attachment,
    ) -> DBResult<LocalAttachmentId> {
        let result = self.create_or_update_attachments(std::iter::once(attachment))?;
        Ok(result[0])
    }

    /// Create or update local attachments from the full attachment metadata.
    ///
    /// # Errors
    /// Returns errors if the query fails.
    pub fn create_or_update_attachments<'i>(
        &mut self,
        attachment: impl IntoIterator<Item = &'i Attachment>,
    ) -> DBResult<Vec<LocalAttachmentId>> {
        let iter = attachment.into_iter();
        let mut result = Vec::with_capacity(iter.size_hint().1.unwrap_or(0));

        let mut stmt = self.0.prepare(indoc! {"
            INSERT INTO attachments (
                rid,
                name,
                size,
                mime_type,
                address_id,
                key_packets,
                signature,
                enc_signature,
                disposition,
                sender,
                conversation_id,
                message_id,
                is_auto_forwardee
            ) VALUES (
                ?,?,?,?,?,?,?,?,?,?,
                (SELECT id FROM conversations WHERE rid=?),
                (SELECT id FROM messages WHERE rid=?),
                ?
            )
            ON CONFLICT (rid) DO UPDATE SET
                key_packets=excluded.key_packets,
                address_id=excluded.address_id,
                signature=excluded.signature,
                enc_signature=excluded.enc_signature,
                sender=excluded.sender,
                conversation_id=excluded.conversation_id,
                message_id=excluded.message_id,
                is_auto_forwardee=excluded.is_auto_forwardee
            RETURNING id
        "})?;

        let mut buffer = JsonWriteBuffer::new();

        for attachment in iter {
            let sender = if let Some(sender) = &attachment.sender {
                Some(buffer.serialize(sender)?)
            } else {
                None
            };
            bind_list_indexed!(
                &mut stmt,
                &attachment.id,
                &attachment.name,
                attachment.size,
                &attachment.mime_type,
                &attachment.address_id,
                &attachment.key_packets.0.as_str(),
                attachment.signature.as_ref().map(|v| v.0.as_str()),
                attachment.enc_signature.as_ref().map(|v| v.0.as_str()),
                &attachment.disposition,
                sender,
                &attachment.conversation_id,
                &attachment.message_id,
                attachment.is_auto_forwardee,
            );
            let local_id: LocalAttachmentId = stmt
                .raw_query()
                .next()?
                .ok_or(proton_sqlite3::rusqlite::Error::QueryReturnedNoRows)
                .and_then(|r| r.get(0))?;

            result.push(local_id);
        }

        Ok(result)
    }

    /// Create or update local attachments from metadata present in Messages.
    ///
    /// Attachment metadata present on messages includes extra information that is not
    /// present in the full attachment info.
    ///
    /// # Errors
    /// Returns errors if the query fails.
    pub fn create_or_update_attachments_from_message<'i>(
        &mut self,
        message_id: LocalMessageId,
        address_id: &AddressId,
        conversation_id: Option<LocalConversationId>,
        attachments: impl IntoIterator<Item = &'i MessageAttachment>,
    ) -> DBResult<Vec<LocalAttachmentId>> {
        let iter = attachments.into_iter();
        let mut result = Vec::with_capacity(iter.size_hint().1.unwrap_or(0));

        let mut stmt = self.0.prepare(indoc! {"
            INSERT INTO attachments (
                rid,
                name,
                size,
                mime_type,
                key_packets,
                signature,
                enc_signature,
                disposition,
                conversation_id,
                message_id,
                content_id,
                transfer_encoding,
                image_width,
                image_height,
                address_id
            ) VALUES (
                ?,?,?,?,?,?,?,?,?,
                ?,?,?,?,?,?
            )
            ON CONFLICT (rid) DO UPDATE SET
                key_packets=excluded.key_packets,
                signature=excluded.signature,
                enc_signature=excluded.enc_signature,
                conversation_id=excluded.conversation_id,
                message_id=excluded.message_id,
                content_id=excluded.content_id,
                transfer_encoding=excluded.transfer_encoding,
                image_width=excluded.image_width,
                image_height=excluded.image_height,
                address_id=excluded.address_id
            RETURNING id
        "})?;

        for attachment in iter {
            bind_list_indexed!(
                &mut stmt,
                &attachment.id,
                &attachment.name,
                attachment.size,
                &attachment.mime_type,
                &attachment.key_packets.0.as_str(),
                attachment.signature.as_ref().map(|v| v.0.as_str()),
                attachment.enc_signature.as_ref().map(|v| v.0.as_str()),
                &attachment.disposition,
                conversation_id,
                message_id,
                &attachment.headers.content_id,
                &attachment.headers.content_transfer_encoding,
                &attachment.headers.image_width,
                &attachment.headers.image_height,
                address_id,
            );
            let local_id: LocalAttachmentId = stmt
                .raw_query()
                .next()?
                .ok_or(proton_sqlite3::rusqlite::Error::QueryReturnedNoRows)
                .and_then(|r| r.get(0))?;

            result.push(local_id);
        }

        Ok(result)
    }

    /// Get an attachment with `id`.
    ///
    /// # Errors
    /// Returns error if the query fails.
    pub fn attachment_with_id(&self, id: LocalAttachmentId) -> DBResult<Option<LocalAttachment>> {
        self.0
            .query_row(
                &LocalAttachmentSelector::query_with_id(),
                [id],
                LocalAttachmentSelector::from_row,
            )
            .optional()
    }

    /// Get all attachments for message with `id`.
    ///
    /// # Errors
    /// Returns error if the query fails.
    pub fn attachments_for_message(&self, id: LocalMessageId) -> DBResult<Vec<LocalAttachment>> {
        let mut stmt = self
            .0
            .prepare(&LocalAttachmentSelector::query_with_message_id())?;
        let rows = stmt.query_map([id], LocalAttachmentSelector::from_row)?;
        mapped_rows_to_vec(rows)
    }

    /// Check whether attachment with `id` is complete.
    ///
    /// Attachment metadata is considered complete when all the information required to
    /// decrypt the attachment is in the database. When storing conversation/messages into the
    /// database we only get partial data for the attachment.
    ///
    /// To complete the data, one needs to provide the full metadata which can be added with
    /// [`create_or_update_attachment`] or [`create_or_update_attachments`].
    ///
    /// # Errors
    /// Return error if the query fails.
    pub fn is_attachment_metadata_complete(
        &self,
        id: LocalAttachmentId,
    ) -> DBResult<Option<(bool, Option<AttachmentId>)>> {
        let result = self
            .0
            .query_row(
                "SELECT key_packets, rid FROM attachments WHERE id=?",
                [id],
                |r| {
                    let v_ref = r.get_ref(0)?;
                    let rid = r.get(1)?;
                    Ok((v_ref.as_str_or_null()?.is_some(), rid))
                },
            )
            .optional()?;
        Ok(result)
    }
}

/// Statement to initialize the attachment table metadata with partial information from the
/// conversation attachment metadata.
pub(crate) struct CreateConversationAttachmentRefStatement<'a>(
    proton_sqlite3::rusqlite::Statement<'a>,
);

impl<'a> CreateConversationAttachmentRefStatement<'a> {
    pub(crate) fn insert(
        &mut self,
        address_id: Option<&AddressId>,
        metadata: &AttachmentMetadata,
        conversation_id: LocalConversationId,
    ) -> DBResult<LocalAttachmentId> {
        self.0.query_row(
            (
                &metadata.id,
                &metadata.name,
                metadata.size,
                &metadata.mime_type,
                metadata.disposition,
                address_id,
                conversation_id,
            ),
            |r| r.get(0),
        )
    }
}

/// Statement to initialize the attachment table metadata with partial information from the
/// message attachment metadata.
pub(crate) struct CreateMessageAttachmentRefStatement<'a>(proton_sqlite3::rusqlite::Statement<'a>);

impl<'a> CreateMessageAttachmentRefStatement<'a> {
    pub(crate) fn insert(
        &mut self,
        address_id: Option<&AddressId>,
        metadata: &AttachmentMetadata,
        message_id: LocalMessageId,
    ) -> DBResult<LocalAttachmentId> {
        self.0.query_row(
            (
                &metadata.id,
                &metadata.name,
                metadata.size,
                &metadata.mime_type,
                metadata.disposition,
                address_id,
                message_id,
            ),
            |r| r.get(0),
        )
    }
}

pub struct LocalAttachmentMetadataSelector {}

impl LocalAttachmentMetadataSelector {
    pub fn query() -> &'static str {
        "SELECT att.id, att.rid, att.name, att.size, att.mime_type, att.disposition FROM attachments AS att"
    }

    pub fn from_row(r: &Row) -> DBResult<LocalAttachmentMetadata> {
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

pub struct LocalAttachmentSelector {}

impl LocalAttachmentSelector {
    pub fn query() -> &'static str {
        indoc! {"
            SELECT
                id,
                rid,
                name,
                size,
                mime_type,
                address_id,
                key_packets,
                signature,
                enc_signature,
                disposition,
                sender,
                message_id,
                conversation_id,
                content_id,
                transfer_encoding,
                image_width,
                image_height
            FROM attachments
        "}
    }

    pub fn query_with_id() -> String {
        format!("{} WHERE id=?", Self::query())
    }

    pub fn query_with_message_id() -> String {
        format!("{} WHERE message_id=?", Self::query())
    }

    pub fn from_row(r: &Row) -> DBResult<LocalAttachment> {
        let mut ridx = RowIndexAllocator::new();
        Ok(LocalAttachment {
            id: r.get(ridx.fetch_and_add())?,
            rid: r.get(ridx.fetch_and_add())?,
            name: r.get(ridx.fetch_and_add())?,
            size: r.get(ridx.fetch_and_add())?,
            mime_type: r.get(ridx.fetch_and_add())?,
            address_id: r.get(ridx.fetch_and_add())?,
            key_packets: r
                .get::<usize, String>(ridx.fetch_and_add())
                .map(KeyPackets::from)?,
            signature: r
                .get::<usize, Option<String>>(ridx.fetch_and_add())
                .map(|v| v.map(AttachmentSignature::from))?,
            encrypted_signature: r
                .get::<usize, Option<String>>(ridx.fetch_and_add())
                .map(|v| v.map(AttachmentEncryptedSignature::from))?,
            disposition: r.get(ridx.fetch_and_add())?,
            sender: deserialize_optional_json_from_row(r, ridx.fetch_and_add())?,
            message_id: r.get(ridx.fetch_and_add())?,
            conversation_id: r.get(ridx.fetch_and_add())?,
            content_id: r.get(ridx.fetch_and_add())?,
            content_transfer_encoding: r.get(ridx.fetch_and_add())?,
            pm_image_width: r.get(ridx.fetch_and_add())?,
            pm_image_height: r.get(ridx.fetch_and_add())?,
        })
    }
}
