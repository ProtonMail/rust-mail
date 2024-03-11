use crate::attachments::LocalAttachmentMetadataSelector;
use crate::conversations::types::{
    LocalConversation, LocalConversationId, LocalConversationWithContext,
};
use crate::json::{deserialize_json_from_row, JsonWriteBuffer};
use crate::{
    DBResult, DeletedState, LabelColor, LocalAttachmentMetadata, LocalConversationCount,
    LocalConversationLabel, LocalLabelId, MailSqliteConnectionImpl,
};
use proton_api_mail::domain::{Conversation, ConversationCount, ConversationId, MessageAddress};
use proton_sqlite3::rusqlite::types::FromSqlError;
use proton_sqlite3::rusqlite::{params_from_iter, OptionalExtension, Row};
use proton_sqlite3::utils::{
    gen_variable_in_argument_list, mapped_rows_into_vec, mapped_rows_to_vec, StmtIndexAllocator,
};
use std::str::FromStr;

impl<'c> MailSqliteConnectionImpl<'c> {
    pub fn create_conversation(
        &mut self,
        conversation: &Conversation,
    ) -> DBResult<LocalConversationId> {
        let result = self.create_conversations(std::iter::once(conversation))?;
        Ok(result[0])
    }

    /// Creates new or updates existing conversations.
    pub fn create_conversations<'i>(
        &mut self,
        conversations: impl ExactSizeIterator<Item = &'i Conversation>,
    ) -> DBResult<Vec<LocalConversationId>> {
        self.create_or_update_conversations(conversations)
    }

    pub fn update_conversation(&mut self, conversation: &Conversation) -> DBResult<()> {
        self.update_conversations(std::iter::once(conversation))
    }
    /// Creates new or updates existing conversations.
    pub fn update_conversations<'i>(
        &mut self,
        conversations: impl ExactSizeIterator<Item = &'i Conversation>,
    ) -> DBResult<()> {
        self.create_or_update_conversations(conversations)?;
        Ok(())
    }

    //TODO: Better update statement.
    fn create_or_update_conversations<'i>(
        &mut self,
        conversations: impl ExactSizeIterator<Item = &'i Conversation>,
    ) -> DBResult<Vec<LocalConversationId>> {
        let mut stmt = self.0.prepare(
            "INSERT INTO conversations (rid, `order`, subject, senders, recipients, num_messages, \
num_unread, num_attachments, expiration_time, size) VALUES (?,?,?,?,?,?,?,?,?,?) ON CONFLICT(rid) DO UPDATE SET \
num_messages=excluded.num_messages, num_attachments=excluded.num_attachments, \
expiration_time=excluded.expiration_time, size=excluded.size RETURNING id",
        )?;

        let mut resolve_conv_id_stmt =
            self.0.prepare("SELECT id FROM conversations WHERE rid=?")?;

        let mut labels_statement = self.0.prepare(&format!(
            "INSERT OR REPLACE INTO conversation_labels (label_id, conversation_id, ctx_time, ctx_size,
ctx_num_messages, ctx_num_unread, ctx_num_attachments) VALUES \
(({RESOLVE_LABEL_ID_STATEMENT}),?,?,?,?,?,?)"
        ))?;

        let mut attachment_to_conv_stmt = self
            .0
            .prepare("INSERT OR IGNORE into conversation_attachments VALUES (?, ?)")?;

        let mut attachments_stmt = self.create_attachment_ref_statement()?;

        let mut senders_buffer = JsonWriteBuffer::new();
        let mut receives_buffers = JsonWriteBuffer::new();

        let mut ids = Vec::with_capacity(conversations.len());

        for conv in conversations {
            let senders = senders_buffer.serialize(&conv.senders)?;
            let recipients = receives_buffers.serialize(&conv.recipients)?;

            let conv_id: LocalConversationId = if let Some(id) = stmt
                .query_row(
                    (
                        &conv.id,
                        &conv.order,
                        &conv.subject,
                        senders,
                        recipients,
                        conv.num_messages,
                        conv.num_unread,
                        conv.num_attachments,
                        conv.expiration_time,
                        conv.size,
                    ),
                    |r| r.get(0),
                )
                .optional()?
            {
                id
            } else {
                resolve_conv_id_stmt.query_row([&conv.id], |r| r.get(0))?
            };

            // Remove any labels that are no longer associated with this conversation.
            if !conv.labels.is_empty() {
                let mut stmt = self.0.prepare(&format!(
                    "DELETE FROM conversation_labels WHERE conversation_id=? \
            AND label_id NOT IN (SELECT id FROM labels WHERE rid IN ({}))",
                    gen_variable_in_argument_list(conv.labels.len())
                ))?;
                let mut row_index = StmtIndexAllocator::new();
                stmt.raw_bind_parameter(row_index.fetch_and_add(), conv_id)?;
                for label in &conv.labels {
                    stmt.raw_bind_parameter(row_index.fetch_and_add(), &label.id)?;
                }
                stmt.raw_execute()?;
            } else {
                self.0.execute(
                    "DELETE FROM conversation_labels WHERE conversation_id=?",
                    [conv_id],
                )?;
            }

            for label in &conv.labels {
                labels_statement.execute((
                    &label.id,
                    conv_id,
                    label.context_time,
                    label.context_size,
                    label.context_num_messages,
                    label.context_num_unread,
                    label.context_num_attachments,
                ))?;
            }

            if !conv.attachments_metadata.is_empty() {
                // Remove any attachments that are no longer associated with this conversation.
                let mut stmt = self.0.prepare(&format!(
                    "DELETE FROM conversation_attachments WHERE conversation_id=? \
            AND attachment_id NOT IN ({})",
                    gen_variable_in_argument_list(conv.attachments_metadata.len())
                ))?;
                let mut row_index = StmtIndexAllocator::new();
                stmt.raw_bind_parameter(row_index.fetch_and_add(), conv_id)?;
                for attachment in &conv.attachments_metadata {
                    stmt.raw_bind_parameter(row_index.fetch_and_add(), &attachment.id)?;
                }
                stmt.raw_execute()?;
            } else {
                self.0.execute(
                    "DELETE FROM conversation_attachments WHERE conversation_id=?",
                    [conv_id],
                )?;
            }
            for attachment in &conv.attachments_metadata {
                if let Some(local_id) = attachments_stmt.insert(None, attachment).optional()? {
                    attachment_to_conv_stmt.execute((conv_id, local_id))?;
                }
            }

            ids.push(conv_id);
        }
        Ok(ids)
    }

    pub fn get_conversation(&self, id: LocalConversationId) -> DBResult<Option<LocalConversation>> {
        self.0
            .query_row(
                &ConversationSelector::query_with_id(),
                [id],
                ConversationSelector::from_row,
            )
            .optional()
    }

    pub fn get_conversations(
        &self,
        ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<Vec<LocalConversation>> {
        let mut stmt = self
            .0
            .prepare(&ConversationSelector::query_with_id_in(ids.len()))?;
        let mut result = Vec::with_capacity(ids.len());
        let r = stmt.query_map(params_from_iter(ids), ConversationSelector::from_row)?;
        mapped_rows_into_vec(&mut result, r)?;
        Ok(result)
    }

    pub fn get_conversation_with_context(
        &self,
        id: LocalConversationId,
        label_id: LocalLabelId,
    ) -> DBResult<Option<LocalConversationWithContext>> {
        self.0
            .query_row(
                &ConversationSelectorWithContext::query_with_id(),
                (label_id, id),
                ConversationSelectorWithContext::from_row,
            )
            .optional()
    }

    pub fn get_conversation_count_with_context(&self, label_id: LocalLabelId) -> DBResult<usize> {
        self.0.query_row(
            "SELECT COUNT(conversation_id) FROM conversation_labels WHERE label_id=?",
            [label_id],
            |r| r.get(0),
        )
    }

    pub fn get_conversation_ids_with_context(
        &self,
        label_id: LocalLabelId,
    ) -> DBResult<Vec<LocalConversationId>> {
        let mut stmt = self
            .0
            .prepare("SELECT (conversation_id) FROM conversation_labels WHERE label_id=?")?;
        let r = stmt.query_map([label_id], |r| r.get(0))?;
        mapped_rows_to_vec(r)
    }

    pub fn get_conversations_with_context(
        &self,
        label_id: LocalLabelId,
        limit: usize,
    ) -> DBResult<Vec<LocalConversationWithContext>> {
        let mut stmt = self
            .0
            .prepare(&ConversationSelectorWithContext::query_with_limit())?;
        let r = stmt.query_map((label_id, limit), ConversationSelectorWithContext::from_row)?;
        mapped_rows_to_vec(r)
    }

    pub fn get_conversations_with_ids_and_context(
        &self,
        label_id: LocalLabelId,
        ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<Vec<LocalConversationWithContext>> {
        let mut stmt = self
            .0
            .prepare(&ConversationSelectorWithContext::query_with_id_in(
                ids.len(),
            ))?;

        stmt.raw_bind_parameter(1, label_id)?;
        for (idx, id) in ids.enumerate() {
            stmt.raw_bind_parameter(idx + 2, id)?;
        }

        let r = stmt
            .raw_query()
            .mapped(ConversationSelectorWithContext::from_row);
        mapped_rows_to_vec(r)
    }

    pub fn get_conversation_attachments(
        &self,
        id: LocalConversationId,
    ) -> DBResult<Option<Vec<LocalAttachmentMetadata>>> {
        let query = format!(
"{} JOIN conversation_attachments ON att.id=conversation_attachments.attachment_id and \
conversation_attachments.conversation_id=?", LocalAttachmentMetadataSelector::query(),
        );

        let mut stmt = self.0.prepare(&query)?;
        let Some(rows) = stmt
            .query_map([id], LocalAttachmentMetadataSelector::from_row)
            .optional()?
        else {
            return Ok(None);
        };

        Ok(Some(mapped_rows_to_vec(rows)?))
    }

    pub fn mark_conversation_as_deleted(
        &mut self,
        id: LocalConversationId,
        deleted: DeletedState,
    ) -> DBResult<()> {
        self.mark_conversations_as_deleted(deleted, std::iter::once(id))
    }

    pub fn mark_conversations_as_deleted(
        &mut self,
        deleted_state: DeletedState,
        ids: impl Iterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let mut stmt = self
            .0
            .prepare("UPDATE conversations SET deleted=? WHERE id=?")?;
        for id in ids {
            stmt.execute((deleted_state, id))?;
        }
        Ok(())
    }

    pub fn mark_remote_conversation_as_deleted(&mut self, id: &ConversationId) -> DBResult<()> {
        self.mark_remote_conversations_as_deleted(std::iter::once(id))
    }

    pub fn mark_remote_conversations_as_deleted<'i>(
        &mut self,
        ids: impl Iterator<Item = &'i ConversationId>,
    ) -> DBResult<()> {
        let mut stmt = self
            .0
            .prepare("UPDATE conversations SET deleted=? WHERE rid=?")?;
        for id in ids {
            stmt.execute((DeletedState::Remote, id))?;
        }
        Ok(())
    }

    pub fn create_or_update_conversation_counts<'i>(
        &mut self,
        counts: impl Iterator<Item = &'i ConversationCount>,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare(
            "INSERT OR REPLACE INTO label_conversation_count VALUES \
        ((SELECT id FROM labels WHERE rid=?),?,?)",
        )?;

        for count in counts {
            stmt.execute((&count.label_id, count.total, count.unread))?;
        }
        Ok(())
    }

    pub fn get_conversation_counts(&self) -> DBResult<Vec<LocalConversationCount>> {
        let mut stmt = self.0.prepare("SELECT * FROM label_conversation_count")?;
        let r = mapped_rows_to_vec(stmt.query_map((), |r| {
            Ok(LocalConversationCount {
                id: r.get(0)?,
                total: r.get(1)?,
                unread: r.get(2)?,
            })
        })?)?;
        Ok(r)
    }
}

const RESOLVE_LABEL_ID_STATEMENT: &str = "SELECT id FROM labels WHERE rid = ?";

struct ConversationSelector {}
impl ConversationSelector {
    fn query() -> &'static str {
        "SELECT id, rid, `order`, subject, senders, recipients, num_messages, \
num_unread, num_attachments, expiration_time, size \
FROM conversations WHERE deleted=0"
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

    fn from_row(r: &Row) -> DBResult<LocalConversation> {
        Ok({
            LocalConversation {
                id: r.get(0)?,
                remote_id: r.get(1)?,
                order: r.get(2)?,
                subject: r.get(3)?,
                senders: deserialize_json_from_row::<Vec<MessageAddress>>(r, 4)?,
                recipients: deserialize_json_from_row::<Vec<MessageAddress>>(r, 5)?,
                num_messages: r.get(6)?,
                num_unread: r.get(7)?,
                num_attachments: r.get(8)?,
                expiration_time: r.get(9)?,
                size: r.get(10)?,
            }
        })
    }
}

const CONVERSATION_SELECTOR_WITH_CONTEXT_ORDER_CLAUSE: &str =
    " ORDER BY CL.ctx_time DESC, C.`order` DESC";
struct ConversationSelectorWithContext {}
impl ConversationSelectorWithContext {
    fn query_base() -> &'static str {
        "SELECT C.id, C.rid, C.`order`, C.subject, C.senders, C.recipients, C.num_messages,  \
C.num_unread, C.num_attachments, C.expiration_time, C.size, \
ifnull(CL.ctx_time,0), ifnull(CL.ctx_size,0), ifnull(CL.ctx_num_messages,0), ifnull(CL.ctx_num_unread,0), \
ifnull(CL.ctx_num_attachments,0), \
GROUP_CONCAT(L.id || ',' || L.name || ',' || L.color, ';')\
FROM conversations AS C \
JOIN conversation_labels AS CL ON CL.conversation_id=C.id AND CL.label_id=? \
LEFT JOIN labels AS L ON L.id = CL.label_id AND L.id IN (SELECT id FROM labels WHERE type=1) \
WHERE C.deleted=0"
    }

    fn query() -> String {
        format!(
            "{} {}",
            Self::query_base(),
            CONVERSATION_SELECTOR_WITH_CONTEXT_ORDER_CLAUSE
        )
    }

    fn query_with_id() -> String {
        format!(
            "{} AND C.id=? {}",
            Self::query_base(),
            CONVERSATION_SELECTOR_WITH_CONTEXT_ORDER_CLAUSE
        )
    }

    fn query_with_id_in(count: usize) -> String {
        format!(
            "{} AND C.id IN ({}) {}",
            Self::query_base(),
            gen_variable_in_argument_list(count),
            CONVERSATION_SELECTOR_WITH_CONTEXT_ORDER_CLAUSE,
        )
    }

    fn query_with_limit() -> String {
        format!("{} LIMIT ?", Self::query())
    }

    fn from_row(r: &Row) -> DBResult<LocalConversationWithContext> {
        Ok(LocalConversationWithContext {
            id: r.get(0)?,
            remote_id: r.get(1)?,
            order: r.get(2)?,
            subject: r.get(3)?,
            senders: deserialize_json_from_row::<Vec<MessageAddress>>(r, 4)?,
            recipients: deserialize_json_from_row::<Vec<MessageAddress>>(r, 5)?,
            num_messages: r.get(6)?,
            num_unread: r.get(7)?,
            num_attachments: r.get(8)?,
            expiration_time: r.get(9)?,
            size: r.get(10)?,
            context_time: r.get(11)?,
            context_size: r.get(12)?,
            context_num_messages: r.get(13)?,
            context_num_unread: r.get(14)?,
            context_num_attachments: r.get(15)?,
            labels: conversation_label_from_row(r, 16)?,
        })
    }
}

fn conversation_label_from_row(
    r: &Row,
    index: usize,
) -> DBResult<Option<Vec<LocalConversationLabel>>> {
    let Some(value) = r.get_ref(index)?.as_str_or_null()? else {
        return Ok(None);
    };

    let mut labels = Vec::new();
    for split in value.split(';') {
        let mut split = split.split(',');
        let Some(split_id) = split.next() else {
            return Err(FromSqlError::InvalidType.into());
        };
        let Ok(label_id) = u64::from_str(split_id) else {
            return Err(FromSqlError::InvalidType.into());
        };

        let Some(name) = split.next() else {
            return Err(FromSqlError::InvalidType.into());
        };
        let label_name = name.to_string();

        let Some(color) = split.next() else {
            return Err(FromSqlError::InvalidType.into());
        };

        let label_color = LabelColor::from(color);
        labels.push(LocalConversationLabel {
            id: LocalLabelId::from(label_id),
            name: label_name,
            color: label_color,
        })
    }

    Ok(Some(labels))
}
