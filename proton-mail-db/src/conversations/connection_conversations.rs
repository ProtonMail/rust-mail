use crate::attachments::LocalAttachmentMetadataSelector;
use crate::conversations::types::{LocalConversation, LocalConversationId};
use crate::json::{deserialize_json_from_row, deserialize_optional_json_from_row, JsonWriteBuffer};
use crate::{
    DBResult, DeletedState, LocalAttachmentMetadata, LocalConversationCount,
    LocalConversationLabel, LocalLabelId, MailSqliteConnectionImpl,
};
use proton_api_mail::domain::{Conversation, ConversationCount, ConversationId, MessageAddress};
use proton_sqlite3::rusqlite::{params_from_iter, OptionalExtension, Row};
use proton_sqlite3::utils::{
    gen_variable_in_argument_list, mapped_rows_into_vec, mapped_rows_to_vec, StmtIndexAllocator,
};
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
num_unread, num_attachments, expiration_time, size, flagged) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(rid) DO UPDATE SET \
num_messages=excluded.num_messages, num_attachments=excluded.num_attachments, \
expiration_time=excluded.expiration_time, size=excluded.size, flagged=excluded.flagged RETURNING id",
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

            let is_starred = conv.is_starred();
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
                        is_starred,
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
                let attachment_id = attachments_stmt.insert(None, attachment)?;
                attachment_to_conv_stmt.execute((conv_id, attachment_id))?;
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
    ) -> DBResult<Option<LocalConversation>> {
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
    ) -> DBResult<Vec<LocalConversation>> {
        let mut stmt = self
            .0
            .prepare(&ConversationSelectorWithContext::query_with_limit())?;
        let r = stmt.query_map((label_id, limit), ConversationSelectorWithContext::from_row)?;
        let conversations = mapped_rows_to_vec(r)?;
        Ok(conversations)
    }

    pub fn get_conversations_with_ids_and_context(
        &self,
        label_id: LocalLabelId,
        ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<Vec<LocalConversation>> {
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
        let conversations = mapped_rows_to_vec(r)?;
        Ok(conversations)
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

    pub fn mark_conversation_as_deleted(&mut self, id: LocalConversationId) -> DBResult<()> {
        self.mark_conversations_as_deleted(std::iter::once(id))
    }

    pub fn mark_conversations_as_deleted(
        &mut self,
        ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare(&format!(
            "UPDATE conversations SET deleted=1 WHERE id IN ({}) AND deleted=0 RETURNING id",
            gen_variable_in_argument_list(ids.len())
        ))?;

        let mut filtered_ids = Vec::with_capacity(ids.len());
        mapped_rows_into_vec(
            &mut filtered_ids,
            stmt.query_map(params_from_iter(ids), |r| r.get(0))?,
        )?;

        // Remove from labels.
        self.remove_conversations_from_labels(&filtered_ids)?;
        // Update message counters
        self.mark_local_messages_as_deleted_with_conversation_ids(filtered_ids.into_iter())?;
        Ok(())
    }

    fn remove_conversations_from_labels(&mut self, ids: &[LocalConversationId]) -> DBResult<()> {
        self.0.execute(&format!(r"UPDATE label_conversation_count AS lcc SET total=total-dm.num_messages, unread=unread-dm.num_unread FROM (
            SELECT cl.label_id, SUM(cl.ctx_num_unread <> 0) AS num_unread, SUM(cl.ctx_num_messages <> 0) AS num_messages FROM conversation_labels AS cl WHERE cl.conversation_id IN ({})
            GROUP BY cl.label_id
        ) AS dm WHERE lcc.label_id=dm.label_id
        ",gen_variable_in_argument_list(ids.len())), params_from_iter(ids.iter()))?;
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
            "INSERT INTO label_conversation_count VALUES \
            ((SELECT id FROM labels WHERE rid=?),?,?) ON CONFLICT (label_id) DO UPDATE SET \
        total=excluded.total, unread=excluded.unread",
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
        r"WITH json_conversation_labels AS (
    SELECT C.conversation_id as cid, json_group_array(json_object('id', L.id, 'name', L.name, 'color', L.color)) as labels
    FROM conversation_labels C
    INNER JOIN labels AS L ON C.label_id = L.id AND L.type=1
    GROUP BY C.conversation_id
),
json_conv_attachments AS (
    SELECT C.conversation_id as cid, json_group_array(json_object('id', A.id, 'rid', A.rid, 'name', A.name,
    'mime_type', A.mime_type, 'disposition', A.disposition, 'size', A.size)) as json_attachments
    FROM conversation_attachments as C
    INNER JOIN attachments AS A ON C.attachment_id = A.id
    GROUP BY C.conversation_id
)

SELECT C.id, C.rid, C.`order`, C.subject, C.senders, C.recipients, C.num_messages,
C.num_unread, C.num_attachments, C.expiration_time, C.size, C.flagged, CLJ.labels, CA.json_attachments
FROM conversations AS C
LEFT JOIN json_conversation_labels AS CLJ ON CLJ.cid = C.id
LEFT JOIN json_conv_attachments AS CA ON CA.cid = C.id
WHERE deleted=0"
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
                num_messages_ctx: 0,
                num_unread: r.get(7)?,
                num_attachments: r.get(8)?,
                expiration_time: r.get(9)?,
                size: r.get(10)?,
                starred: r.get(11)?,
                labels: deserialize_optional_json_from_row::<Vec<LocalConversationLabel>>(r, 12)?,
                time: 0,
                attachments: deserialize_optional_json_from_row::<Vec<LocalAttachmentMetadata>>(
                    r, 13,
                )?,
            }
        })
    }
}

const CONVERSATION_SELECTOR_WITH_CONTEXT_ORDER_CLAUSE: &str =
    " GROUP BY C.id ORDER BY CL.ctx_time DESC, C.`order` DESC ";
struct ConversationSelectorWithContext {}
impl ConversationSelectorWithContext {
    fn query_base() -> &'static str {
        r"WITH json_conversation_labels AS (
    SELECT C.conversation_id as cid, json_group_array(json_object('id', L.id, 'name', L.name, 'color', L.color)) as labels
    FROM conversation_labels C
    INNER JOIN labels AS L ON C.label_id = L.id AND L.type=1
    GROUP BY C.conversation_id
),
json_conv_attachments AS (
    SELECT C.conversation_id as cid, json_group_array(json_object('id', A.id, 'rid', A.rid, 'name', A.name,
    'mime_type', A.mime_type, 'disposition', A.disposition, 'size', A.size)) as json_attachments
    FROM conversation_attachments as C
    INNER JOIN attachments AS A ON C.attachment_id = A.id
    GROUP BY C.conversation_id
)

SELECT C.id, C.rid, C.`order`, C.subject, C.senders, C.recipients, C.expiration_time,
ifnull(CL.ctx_time,0), ifnull(CL.ctx_size,0), ifnull(CL.ctx_num_messages,0), ifnull(CL.ctx_num_unread,0),
ifnull(CL.ctx_num_attachments,0), C.flagged, CLJ.labels, CA.json_attachments, C.num_messages
FROM conversations AS C
INNER JOIN conversation_labels AS CL ON CL.conversation_id=C.id AND CL.label_id=?
LEFT JOIN json_conversation_labels AS CLJ ON CLJ.cid = C.id
LEFT JOIN json_conv_attachments AS CA ON CA.cid = C.id
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

    fn from_row(r: &Row) -> DBResult<LocalConversation> {
        Ok(LocalConversation {
            id: r.get(0)?,
            remote_id: r.get(1)?,
            order: r.get(2)?,
            subject: r.get(3)?,
            senders: deserialize_json_from_row::<Vec<MessageAddress>>(r, 4)?,
            recipients: deserialize_json_from_row::<Vec<MessageAddress>>(r, 5)?,
            expiration_time: r.get(6)?,
            time: r.get(7)?,
            size: r.get(8)?,
            num_messages_ctx: r.get(9)?,
            num_unread: r.get(10)?,
            num_attachments: r.get(11)?,
            starred: r.get(12)?,
            labels: deserialize_optional_json_from_row::<Vec<LocalConversationLabel>>(r, 13)?,
            attachments: deserialize_optional_json_from_row::<Vec<LocalAttachmentMetadata>>(r, 14)?,
            num_messages: r.get(15)?,
        })
    }
}
