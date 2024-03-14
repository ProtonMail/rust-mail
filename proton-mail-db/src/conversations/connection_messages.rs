use crate::json::{deserialize_json_from_row, JsonWriteBuffer};
use crate::{
    DBResult, DeletedState, LocalLabelId, LocalMessageCount, LocalMessageId, LocalMessageMetadata,
    MailSqliteConnectionImpl,
};
use proton_api_mail::domain::{LabelId, MessageAddress, MessageCount, MessageId, MessageMetadata};
use proton_sqlite3::rusqlite::{params_from_iter, OptionalExtension, Row, Statement};
use proton_sqlite3::utils::{
    gen_variable_in_argument_list, mapped_rows_into_vec, mapped_rows_to_vec, StmtIndexAllocator,
};

impl<'c> MailSqliteConnectionImpl<'c> {
    pub fn create_message_from_metadata(
        &mut self,
        metadata: &MessageMetadata,
    ) -> DBResult<LocalMessageId> {
        let r = self.create_messages_from_metadata(std::iter::once(metadata))?;
        Ok(r[0])
    }

    pub fn create_messages_from_metadata<'i>(
        &mut self,
        metadata: impl ExactSizeIterator<Item = &'i MessageMetadata>,
    ) -> DBResult<Vec<LocalMessageId>> {
        self.create_or_update_messages_from_metadata(metadata)
    }

    pub fn update_message_from_metadata(&mut self, metadata: &MessageMetadata) -> DBResult<()> {
        self.update_messages_from_metadata(std::iter::once(metadata))
    }

    pub fn update_messages_from_metadata<'i>(
        &mut self,
        metadata: impl ExactSizeIterator<Item = &'i MessageMetadata>,
    ) -> DBResult<()> {
        self.create_or_update_messages_from_metadata(metadata)?;
        Ok(())
    }

    fn create_or_update_messages_from_metadata<'i>(
        &mut self,
        metadata: impl ExactSizeIterator<Item = &'i MessageMetadata>,
    ) -> DBResult<Vec<LocalMessageId>> {
        let mut to_list_buffer = JsonWriteBuffer::new();
        let mut cc_list_buffer = JsonWriteBuffer::new();
        let mut bcc_list_buffer = JsonWriteBuffer::new();

        let mut result = Vec::with_capacity(metadata.len());

        let mut label_stmt = self.0.prepare(
            "INSERT OR IGNORE INTO message_labels VALUES (?, (SELECT id FROM labels WHERE rid=?))",
        )?;
        let mut msg_stmt = self.0.prepare(&create_or_update_message_query())?;
        let mut message_to_attachment_stmt = self
            .0
            .prepare("INSERT OR IGNORE into message_attachments VALUES (?,?)")?;
        let mut attachment_stmt = self.create_attachment_ref_statement()?;

        for metadata in metadata {
            bind_message_metadata_create(
                &mut msg_stmt,
                metadata,
                &mut to_list_buffer,
                &mut cc_list_buffer,
                &mut bcc_list_buffer,
            )?;
            let local_id: LocalMessageId = msg_stmt
                .raw_query()
                .next()
                .unwrap()
                .ok_or(proton_sqlite3::rusqlite::Error::QueryReturnedNoRows)
                .and_then(|r| r.get(0))?;

            if !metadata.label_ids.is_empty() {
                let mut stmt = self.0.prepare(
                    &format!("DELETE FROM message_labels WHERE message_id=? AND label_id NOT IN (SELECT id FROM labels WHERE rid IN ({}))", gen_variable_in_argument_list(metadata.label_ids.len())))?;
                let mut row_alloc = StmtIndexAllocator::new();
                stmt.raw_bind_parameter(row_alloc.fetch_and_add(), local_id)?;
                for label_id in &metadata.label_ids {
                    stmt.raw_bind_parameter(row_alloc.fetch_and_add(), label_id)?;
                }
                stmt.raw_execute()?;
            } else {
                self.0
                    .execute("DELETE FROM message_labels WHERE message_id=?", [local_id])?;
            }

            // TODO: single select query?
            for label in &metadata.label_ids {
                label_stmt.execute((local_id, &label))?;
            }

            if !metadata.attachments_metadata.is_empty() {
                // Remove any attachments that are no longer associated with this conversation.
                let mut stmt = self.0.prepare(&format!(
                    "DELETE FROM message_attachments WHERE message_id=? \
            AND attachment_id NOT IN ({})",
                    gen_variable_in_argument_list(metadata.attachments_metadata.len())
                ))?;
                let mut row_index = StmtIndexAllocator::new();
                stmt.raw_bind_parameter(row_index.fetch_and_add(), &metadata.id)?;
                for attachment in &metadata.attachments_metadata {
                    stmt.raw_bind_parameter(row_index.fetch_and_add(), &attachment.id)?;
                }
                stmt.raw_execute()?;
            } else {
                self.0.execute(
                    "DELETE FROM message_attachments WHERE message_id=?",
                    [local_id],
                )?;
            }

            for attachment in &metadata.attachments_metadata {
                if let Some(attachment_id) = attachment_stmt
                    .insert(Some(&metadata.address_id), attachment)
                    .optional()?
                {
                    message_to_attachment_stmt.execute((local_id, attachment_id))?;
                }
            }

            result.push(local_id);
        }
        Ok(result)
    }

    pub fn get_message_metadata(
        &self,
        id: LocalMessageId,
    ) -> DBResult<Option<LocalMessageMetadata>> {
        self.0
            .query_row(
                &LocalMessageMetadataSelector::query_with_id(),
                [id],
                LocalMessageMetadataSelector::from_row,
            )
            .optional()
    }

    pub fn get_messages_metadata(
        &self,
        ids: impl ExactSizeIterator<Item = LocalMessageId>,
    ) -> DBResult<Vec<LocalMessageMetadata>> {
        let mut result = Vec::with_capacity(ids.len());
        let mut stmt = self
            .0
            .prepare(&LocalMessageMetadataSelector::query_with_id_in(ids.len()))?;
        let r = stmt.query_map(
            params_from_iter(ids),
            LocalMessageMetadataSelector::from_row,
        )?;
        mapped_rows_into_vec(&mut result, r)?;
        Ok(result)
    }

    pub fn get_message_labels(&self, id: LocalMessageId) -> DBResult<Option<Vec<LocalLabelId>>> {
        if let Some(r) = self
            .0
            .prepare("SELECT label_id FROM message_labels WHERE message_id =?")?
            .query_map([id], |r| r.get(0))
            .optional()?
        {
            return Ok(Some(mapped_rows_to_vec(r)?));
        }

        Ok(None)
    }

    pub fn create_or_update_message_counts<'i>(
        &mut self,
        counts: impl Iterator<Item = &'i MessageCount>,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare(
            "INSERT OR REPLACE INTO label_message_count VALUES \
        ((SELECT id FROM labels WHERE rid=?),?,?)",
        )?;

        for count in counts {
            stmt.execute((&count.label_id, count.total, count.unread))?;
        }
        Ok(())
    }

    pub fn get_message_counts(&self) -> DBResult<Vec<LocalMessageCount>> {
        let mut stmt = self.0.prepare("SELECT * FROM label_message_count")?;
        let r = mapped_rows_to_vec(stmt.query_map((), |r| {
            Ok(LocalMessageCount {
                id: r.get(0)?,
                total: r.get(1)?,
                unread: r.get(2)?,
            })
        })?)?;
        Ok(r)
    }

    pub fn mark_remote_message_as_deleted(&mut self, id: &MessageId) -> DBResult<()> {
        self.0.execute(
            "UPDATE messages SET delete=? WHERE rid=?",
            (DeletedState::Remote, id),
        )?;
        Ok(())
    }
}

macro_rules! bind_list {
    ($stmt:ident, $($exp:expr,)+ $(,)?) => {
        bind_list_ordered!(1, $stmt, $($exp),+);
    };
}

macro_rules! bind_list_ordered {
    ($index:tt, $stmt:ident, $exp:expr $(,)?) => {
        $stmt.raw_bind_parameter($index,$exp)?;
    };

    ($index:tt, $stmt:ident, $exp:expr $(,$r:expr)+) => {
        $stmt.raw_bind_parameter($index, $exp)?;
        bind_list_ordered!(($index+1),$stmt $(,$r)+)
    };
}

fn create_or_update_message_query() -> String {
    format!(
        r"INSERT INTO messages (
    conversation_id, rid, address_id, `order`, subject, unread,
    sender_address, sender_name, sender_is_proton, sender_is_simple_login, sender_bimi_selector,
    sender_display_image, to_list, cc_list, bcc_list, time, size, expiration_time,
    is_replied, is_replied_all, is_forwarded, external_id, num_attachments, flags, flagged
) VALUES  ((SELECT id FROM conversations WHERE rid=?),{})
ON CONFLICT(rid) DO UPDATE SET
    conversation_id = excluded.conversation_id,
    address_id=excluded.address_id,
    `order`=excluded.`order`,
    subject=excluded.subject,
    unread=excluded.unread,
    sender_address=excluded.sender_address,
    sender_name=excluded.sender_name,
    sender_is_proton=excluded.sender_is_proton,
    sender_is_simple_login=excluded.sender_is_simple_login,
    sender_bimi_selector=excluded.sender_bimi_selector,
    sender_display_image=excluded.sender_display_image,
    to_list=excluded.to_list,
    cc_list=excluded.cc_list,
    bcc_list=excluded.bcc_list,
    time=excluded.time,
    size=excluded.size,
    expiration_time=excluded.expiration_time,
    is_replied=excluded.is_replied,
    is_replied_all=excluded.is_replied_all,
    is_forwarded=excluded.is_forwarded,
    external_id=excluded.external_id,
    num_attachments=excluded.num_attachments,
    flags=excluded.flags,
    flagged=excluded.flagged
RETURNING id",
        gen_variable_in_argument_list(24)
    )
}

fn bind_message_metadata_create(
    stmt: &mut Statement,
    m: &MessageMetadata,
    to_list_buffer: &mut JsonWriteBuffer,
    cc_list_buffer: &mut JsonWriteBuffer,
    bcc_list_buffer: &mut JsonWriteBuffer,
) -> DBResult<()> {
    let to_list = to_list_buffer.serialize(&m.to_list)?;
    let cc_list = cc_list_buffer.serialize(&m.cc_list)?;
    let bcc_list = bcc_list_buffer.serialize(&m.bcc_list)?;

    bind_list! {
        stmt,
        &m.conversation_id,
        &m.id,
        &m.address_id,
        m.order,
        &m.subject,
        m.unread,
        &m.sender.address,
        &m.sender.name,
        &m.sender.is_proton,
        &m.sender.is_simple_login,
        &m.sender.bimi_selector,
        &m.sender.display_sender_image,
        to_list,
        cc_list,
        bcc_list,
        m.time,
        m.size,
        m.expiration_time,
        m.is_replied,
        m.is_replied_all,
        m.is_forwarded,
        &m.external_id,
        m.num_attachments,
        m.flags,
        m.label_ids.contains(LabelId::starred()),
    }

    Ok(())
}

struct LocalMessageMetadataSelector {}
impl LocalMessageMetadataSelector {
    fn query() -> &'static str {
        "SELECT id, rid, address_id, conversation_id, `order`, subject, unread, \
sender_address, sender_name, sender_is_proton, sender_is_simple_login, sender_bimi_selector, sender_display_image, \
to_list, cc_list, bcc_list, time, size, expiration_time, \
is_replied, is_replied_all, is_forwarded, external_id, num_attachments, flags, flagged FROM messages WHERE deleted=0"
    }

    fn query_with_id() -> String {
        format!("{} AND id=?", Self::query())
    }

    fn query_with_id_in(count: usize) -> String {
        format!(
            "{} AND id IN ({})",
            Self::query(),
            gen_variable_in_argument_list(count)
        )
    }

    fn from_row(r: &Row) -> DBResult<LocalMessageMetadata> {
        Ok(LocalMessageMetadata {
            id: r.get(0)?,
            rid: r.get(1)?,
            address_id: r.get(2)?,
            conversation_id: r.get(3)?,
            order: r.get(4)?,
            subject: r.get(5)?,
            unread: r.get(6)?,
            sender: MessageAddress {
                address: r.get(7)?,
                name: r.get(8)?,
                is_proton: r.get(9)?,
                is_simple_login: r.get(10)?,
                bimi_selector: r.get(11)?,
                display_sender_image: r.get(12)?,
            },
            to: deserialize_json_from_row(r, 13)?,
            cc: deserialize_json_from_row(r, 14)?,
            bcc: deserialize_json_from_row(r, 15)?,
            time: r.get(16)?,
            size: r.get(17)?,
            expiration_time: r.get(18)?,
            is_replied: r.get(19)?,
            is_replied_all: r.get(20)?,
            is_forwarded: r.get(21)?,
            external_id: r.get(22)?,
            num_attachments: r.get(23)?,
            flags: r.get(24)?,
            starred: r.get(25)?,
        })
    }
}
