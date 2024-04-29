use crate::db::attachments::LocalAttachmentMetadataSelector;
use crate::db::conversations::types::{LocalConversation, LocalConversationId};
use crate::db::json::{
    deserialize_json_from_row, deserialize_optional_json_from_row, JsonWriteBuffer,
};
use crate::db::{
    ConversationAvatarInformation, DBResult, LocalAttachmentMetadata, LocalConversationCount,
    LocalInlineLabelInfo, LocalLabelId, LocalMessageId, MailSqliteConnectionImpl,
};
use proton_api_mail::domain::{
    Conversation, ConversationCount, ConversationId, LabelId, MessageAddress,
};
use proton_sqlite3::rusqlite::{params_from_iter, OptionalExtension, Row};
use proton_sqlite3::utils::{
    gen_variable_in_argument_list, mapped_rows_into_vec, mapped_rows_to_vec, StmtIndexAllocator,
};
use std::collections::BTreeSet;

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

    fn create_or_update_conversations<'i>(
        &mut self,
        conversations: impl ExactSizeIterator<Item = &'i Conversation>,
    ) -> DBResult<Vec<LocalConversationId>> {
        let mut stmt = self.0.prepare(
            r"INSERT INTO conversations (
                rid,
                `order`,
                subject,
                senders,
                recipients,
                num_messages,
                num_unread,
                num_attachments,
                expiration_time,
                size
        ) VALUES (?,?,?,?,?,?,?,?,?,?)
        ON CONFLICT(rid) DO UPDATE SET
            num_messages=excluded.num_messages,
            num_attachments=excluded.num_attachments,
            num_unread=excluded.num_unread,
            expiration_time=excluded.expiration_time,
            size=excluded.size
       RETURNING id",
        )?;

        let mut resolve_conv_id_stmt =
            self.0.prepare("SELECT id FROM conversations WHERE rid=?")?;

        let mut labels_statement = self.0.prepare(&format!(
            r"INSERT OR REPLACE INTO conversation_labels (
                label_id,
                conversation_id,
                ctx_time,
                ctx_size,
                ctx_num_messages,
                ctx_num_unread,
                ctx_num_attachments,
                ctx_expiration_time,
                ctx_snooze_time
            ) VALUES
                (({RESOLVE_LABEL_ID_STATEMENT}),?,?,?,?,?,?,?,?)"
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
                    label.context_expiration_time,
                    label.context_snooze_time,
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

    pub fn mark_conversation_as_deleted(
        &mut self,
        label_id: LocalLabelId,
        id: LocalConversationId,
    ) -> DBResult<()> {
        self.mark_conversations_as_deleted(label_id, std::iter::once(id))
    }

    pub fn mark_conversations_as_deleted(
        &mut self,
        label_id: LocalLabelId,
        ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let mut conv_ids = Vec::with_capacity(ids.len());
        conv_ids.extend(ids);

        // Update message counters
        self.mark_local_messages_as_deleted_with_conversation_ids(
            label_id,
            conv_ids.iter().cloned(),
        )?;

        // Remove from labels.
        self.remove_conversations_from_label(label_id, &conv_ids)?;

        Ok(())
    }

    pub fn unmark_conversation_as_deleted(
        &mut self,
        label_id: LocalLabelId,
        id: LocalConversationId,
    ) -> DBResult<()> {
        self.unmark_conversations_as_deleted(label_id, std::iter::once(id))
    }

    pub fn unmark_conversations_as_deleted(
        &mut self,
        label_id: LocalLabelId,
        ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let mut conv_ids = Vec::with_capacity(ids.len());
        conv_ids.extend(ids);

        // Update message counters
        self.unmark_local_messages_as_deleted_with_conversation_ids(
            label_id,
            conv_ids.iter().cloned(),
        )?;

        // Add to label.
        self.add_conversations_to_label(label_id, &conv_ids)?;

        Ok(())
    }

    fn remove_conversations_from_label(
        &mut self,
        label_id: LocalLabelId,
        ids: &[LocalConversationId],
    ) -> DBResult<()> {
        self.add_or_remove_conversations_from_label(label_id, ids, true)
    }

    fn add_conversations_to_label(
        &mut self,
        label_id: LocalLabelId,
        ids: &[LocalConversationId],
    ) -> DBResult<()> {
        self.add_or_remove_conversations_from_label(label_id, ids, false)
    }

    fn add_or_remove_conversations_from_label(
        &mut self,
        label_id: LocalLabelId,
        ids: &[LocalConversationId],
        delete: bool,
    ) -> DBResult<()> {
        // If the label is all mail we need to delete all the messages. However, since it is possible that some
        // conversations do not have all the message synced, we have to use a different path.
        if let Some(all_mail_label_id) = self.resolve_remote_label_id(LabelId::all_mail())? {
            if label_id == all_mail_label_id {
                return if delete {
                    self.mark_conversations_as_deleted_all_mail(ids.iter().cloned())
                } else {
                    self.unmark_conversations_as_deleted_all_mail(ids.iter().cloned())
                };
            }
        }

        assert!(ids.len() < 512);
        let operator = if delete { '-' } else { '+' };

        let conv_args = gen_variable_in_argument_list(ids.len());

        // Update conversation label.
        if !delete {
            let mut stmt = self.0.prepare(&format!(
                r"
WITH conv_messages AS (
    SELECT m.conversation_id,
    MAX(m.time) AS time,
    MAX(m.expiration_time) AS expiration_time,
    MAX(m.snooze_time) AS snooze_time,
    COUNT(m.id) AS `count`,
    SUM(m.unread) AS unread,
    SUM(m.num_attachments) AS attachments,
    SUM(m.size) AS size
    FROM messages AS m
    JOIN message_labels AS l ON l.message_id=m.id AND l.label_id=?1
    WHERE m.deleted=0 AND m.conversation_id IN ({})
    GROUP BY m.conversation_id
)
INSERT OR REPLACE INTO conversation_labels (
    label_id,
    conversation_id,
    ctx_time,
    ctx_size,
    ctx_num_messages,
    ctx_num_unread,
    ctx_num_attachments,
    ctx_expiration_time,
    ctx_snooze_time
)
SELECT
    ?1,
    conversation_id,
    time,
    size,
    count,
    unread,
    attachments,
    expiration_time,
    snooze_time
FROM conv_messages",
                conv_args
            ))?;
            let mut alloc = StmtIndexAllocator::new();
            stmt.raw_bind_parameter(alloc.fetch_and_add(), label_id)?;
            for id in ids {
                stmt.raw_bind_parameter(alloc.fetch_and_add(), id)?;
            }
            stmt.raw_execute()?;
        }

        // Update conversation counts
        let mut stmt = self.0.prepare(&format!(r"
UPDATE label_conversation_count AS lcc SET total=total{operator}dm.num_messages, unread=unread{operator}dm.num_unread FROM (
    SELECT cl.label_id, SUM(cl.ctx_num_unread <> 0) AS num_unread, SUM(cl.ctx_num_messages <> 0) AS num_messages
    FROM conversation_labels AS cl
    WHERE cl.label_id=? AND cl.conversation_id IN ({})
    GROUP BY cl.label_id
) AS dm WHERE lcc.label_id=dm.label_id
        ",conv_args))?;

        let mut alloc = StmtIndexAllocator::new();
        stmt.raw_bind_parameter(alloc.fetch_and_add(), label_id)?;
        for id in ids {
            stmt.raw_bind_parameter(alloc.fetch_and_add(), id)?;
        }
        stmt.raw_execute()?;

        // Update conversation label. Can't be deleted since it's necessary for the
        // update conversation count at the moment.
        if delete {
            let mut stmt = self.0.prepare(&format!(
                r"DELETE FROM conversation_labels WHERE label_id=? AND conversation_id IN ({})",
                conv_args
            ))?;
            let mut alloc = StmtIndexAllocator::new();
            stmt.raw_bind_parameter(alloc.fetch_and_add(), label_id)?;
            for id in ids {
                stmt.raw_bind_parameter(alloc.fetch_and_add(), id)?;
            }
            stmt.raw_execute()?;
        }

        // Finally if all conversation_labels are deleted mark message as deleted or
        self.0.execute(
            &format!(
                r"
WITH conv_ids AS (SELECT * FROM (VALUES {})),
cte AS (
    SELECT cl.conversation_id, 0==SUM(cl.ctx_num_messages) AS `deleted`,
           SUM(cl.ctx_num_messages) AS `count`, SUM(cl.ctx_num_unread) AS unread,
           SUM(cl.ctx_size) AS size, SUM(cl.ctx_num_attachments) AS num_attachments
    FROM conversation_labels as cl
    JOIN conv_ids ON cl.conversation_id=conv_ids.column1
    GROUP BY cl.conversation_id
)
UPDATE conversations SET deleted=diff.deleted, num_messages=diff.`count`, num_unread=diff.unread,
size=diff.size, num_attachments=diff.num_attachments
FROM (
    SELECT df.column1 AS `conversation_id`,
       IFNULL(cte.deleted, 1) AS `deleted`, IFNULL(cte.count,0) AS `count`,
       IFNULL(cte.unread, 0) AS `unread`, IFNULL(cte.size, 0) AS `size`,
       IFNULL(cte.num_attachments, 0) as `num_attachments`
    FROM conv_ids as df
    LEFT JOIN cte ON df.column1=cte.conversation_id
) as diff WHERE id = diff.conversation_id",
                std::iter::repeat("(?)")
                    .take(ids.len())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            params_from_iter(ids),
        )?;
        Ok(())
    }

    fn mark_conversations_as_deleted_all_mail(
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
        self.remove_conversations_from_all_labels(&filtered_ids)?;
        // Update message counters
        self.mark_local_messages_as_deleted_with_conversation_ids_all_mail(
            filtered_ids.into_iter(),
        )?;
        Ok(())
    }

    fn unmark_conversations_as_deleted_all_mail(
        &mut self,
        ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare(&format!(
            "UPDATE conversations SET deleted=0 WHERE id IN ({}) AND deleted=1 RETURNING id",
            gen_variable_in_argument_list(ids.len())
        ))?;

        let mut filtered_ids = Vec::with_capacity(ids.len());
        mapped_rows_into_vec(
            &mut filtered_ids,
            stmt.query_map(params_from_iter(ids), |r| r.get(0))?,
        )?;

        // Remove from labels.
        self.add_conversations_to_all_labels(&filtered_ids)?;
        // Update message counters
        self.unmark_local_messages_as_deleted_with_conversation_ids_all_mail(
            filtered_ids.into_iter(),
        )?;
        Ok(())
    }
    fn remove_conversations_from_all_labels(
        &mut self,
        ids: &[LocalConversationId],
    ) -> DBResult<()> {
        self.0.execute(&format!(r"UPDATE label_conversation_count AS lcc SET total=total-dm.num_messages, unread=unread-dm.num_unread FROM (
            SELECT
                cl.label_id,
                SUM(cl.ctx_num_unread <> 0) AS num_unread,
                SUM(cl.ctx_num_messages <> 0) AS num_messages
            FROM conversation_labels AS cl WHERE cl.conversation_id IN ({})
            GROUP BY cl.label_id
        ) AS dm WHERE lcc.label_id=dm.label_id
        ",gen_variable_in_argument_list(ids.len())), params_from_iter(ids.iter()))?;
        Ok(())
    }

    fn add_conversations_to_all_labels(&mut self, ids: &[LocalConversationId]) -> DBResult<()> {
        self.0.execute(&format!(r"UPDATE label_conversation_count AS lcc SET total=total+dm.num_messages, unread=unread+dm.num_unread FROM (
            SELECT
                cl.label_id,
                SUM(cl.ctx_num_unread <> 0) AS num_unread,
                SUM(cl.ctx_num_messages <> 0) AS num_messages
            FROM conversation_labels AS cl WHERE cl.conversation_id IN ({})
            GROUP BY cl.label_id
        ) AS dm WHERE lcc.label_id=dm.label_id
        ",gen_variable_in_argument_list(ids.len())), params_from_iter(ids.iter()))?;
        Ok(())
    }

    pub fn delete_remote_conversation(&mut self, id: &ConversationId) -> DBResult<()> {
        self.delete_remote_conversations(std::iter::once(id))
    }

    pub fn delete_remote_conversations<'i>(
        &mut self,
        ids: impl Iterator<Item = &'i ConversationId>,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare("DELETE FROM conversations WHERE rid=?")?;
        for id in ids {
            stmt.execute([id])?;
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

    pub fn local_to_remote_conversation_ids(
        &self,
        ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<Vec<ConversationId>> {
        assert!(ids.len() < 512);
        let mut stmt = self.0.prepare(&format!(
            "SELECT rid FROM conversations WHERE id IN ({})",
            gen_variable_in_argument_list(ids.len())
        ))?;

        let mut result = Vec::with_capacity(ids.len());
        mapped_rows_into_vec(
            &mut result,
            stmt.query_map(params_from_iter(ids), |r| r.get(0))?,
        )?;
        Ok(result)
    }

    pub fn remote_to_local_conversation_ids<'i>(
        &self,
        ids: impl ExactSizeIterator<Item = &'i ConversationId>,
    ) -> DBResult<Vec<LocalConversationId>> {
        assert!(ids.len() < 512);
        let mut stmt = self.0.prepare(&format!(
            "SELECT id FROM conversations WHERE rid IN ({})",
            gen_variable_in_argument_list(ids.len())
        ))?;

        let mut result = Vec::with_capacity(ids.len());
        mapped_rows_into_vec(
            &mut result,
            stmt.query_map(params_from_iter(ids), |r| r.get(0))?,
        )?;
        Ok(result)
    }

    pub fn mark_conversation_read(&mut self, id: LocalConversationId) -> DBResult<()> {
        self.mark_conversations_read(std::iter::once(id))
    }

    pub fn mark_conversations_read(
        &mut self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let mut ids = Vec::from_iter(ids);
        assert!(ids.len() < 512);
        let conv_args = gen_variable_in_argument_list(ids.len());
        // filter all conversations which are unread

        let mut filter_stmt = self.0.prepare(&format!(
            "UPDATE conversations SET num_unread=0 WHERE id IN ({}) AND num_unread<>0 RETURNING ID",
            conv_args
        ))?;

        let row = filter_stmt.query_map(params_from_iter(&ids), |r| r.get(0))?;
        ids.clear();
        mapped_rows_into_vec(&mut ids, row)?;

        if ids.is_empty() {
            return Ok(());
        }

        let conv_args = gen_variable_in_argument_list(ids.len());

        // Update unread conversation count
        self.0.execute(
            &format!(
                r"
WITH conv_labels AS (
    SELECT label_id, SUM(ctx_num_unread<>0) AS num_unread
    FROM conversation_labels WHERE conversation_id IN ({})
    GROUP BY label_id
)
UPDATE label_conversation_count SET unread=unread-conv_labels.num_unread
FROM conv_labels
WHERE label_conversation_count.label_id = conv_labels.label_id",
                conv_args
            ),
            params_from_iter(&ids),
        )?;

        // Set all conversation label contexts ctx_num_read to 0
        self.0.execute(
            &format!(
                "UPDATE conversation_labels SET ctx_num_unread=0 WHERE conversation_id IN ({})",
                conv_args
            ),
            params_from_iter(&ids),
        )?;

        // Mark all messages with conversation as read
        let mut msg_ids: Vec<LocalMessageId> = Vec::with_capacity(ids.len());
        {
            let mut msg_stmt = self.0.prepare(&format!("UPDATE messages SET unread=0 WHERE conversation_id IN ({}) AND unread<>0 RETURNING id", conv_args))?;
            mapped_rows_into_vec(
                &mut msg_ids,
                msg_stmt.query_map(params_from_iter(&ids), |r| r.get(0))?,
            )?;
        }

        if msg_ids.is_empty() {
            return Ok(());
        }

        // Update message counts
        self.0.execute(
            &format!(
                r"
WITH msg_labels AS (
    SELECT label_id, COUNT(message_id) AS diff_unread
    FROM message_labels
    WHERE message_id IN ({})
    GROUP BY label_id
)

UPDATE label_message_count SET unread=unread-msg_labels.diff_unread
FROM msg_labels
WHERE label_message_count.label_id = msg_labels.label_id
        ",
                gen_variable_in_argument_list(msg_ids.len())
            ),
            params_from_iter(msg_ids),
        )?;
        Ok(())
    }

    pub fn mark_conversation_unread(
        &mut self,
        active_label_id: LocalLabelId,
        id: LocalConversationId,
    ) -> DBResult<()> {
        self.mark_conversations_unread(active_label_id, std::iter::once(id))
    }

    pub fn mark_conversations_unread(
        &mut self,
        active_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let mut ids = BTreeSet::from_iter(ids);

        // Get message and conversation ids
        let mut msg_update_stmt = self.0.prepare(&format!(
            r"
WITH conv_msgs AS (
    SELECT id, MAX(time)
    FROM messages
    WHERE unread=0 AND conversation_id IN ({})
)
UPDATE messages SET unread=1
FROM conv_msgs
WHERE messages.id = conv_msgs.id
RETURNING id, conversation_id
",
            gen_variable_in_argument_list(ids.len())
        ))?;

        let mut msg_pairs: Vec<(LocalMessageId, LocalConversationId)> =
            Vec::with_capacity(ids.len());
        mapped_rows_into_vec(
            &mut msg_pairs,
            msg_update_stmt.query_map(params_from_iter(&ids), |r| Ok((r.get(0)?, r.get(1)?)))?,
        )?;

        for (_, id) in &msg_pairs {
            ids.remove(id);
        }

        // These conversations where asked to b marked as read, but had no messages. Either the
        // messages were already mark as read or there was no metadata. For these we need
        // to set the unread count to 1 and update the current label count. We let the event loop
        // take care of the rest.
        if !ids.is_empty() {
            let args = gen_variable_in_argument_list(ids.len());
            self.0.execute(
                &format!(
                    "UPDATE conversations SET num_unread=num_unread+1 WHERE id IN ({})",
                    args
                ),
                params_from_iter(&ids),
            )?;
            {
                let mut alloc = StmtIndexAllocator::new();
                let mut stmt = self.0.prepare(&format!("UPDATE conversation_labels SET ctx_num_unread=ctx_num_unread+1 WHERE label_id=? AND conversation_id IN ({})", args))?;
                stmt.raw_bind_parameter(alloc.fetch_and_add(), active_label_id)?;
                for id in &ids {
                    stmt.raw_bind_parameter(alloc.fetch_and_add(), id)?;
                }
                stmt.raw_execute()?;
            }
            self.0.execute(
                "UPDATE label_conversation_count SET unread=unread+? WHERE label_id=?",
                (ids.len(), active_label_id),
            )?;
        }

        if msg_pairs.is_empty() {
            return Ok(());
        }

        let args = gen_variable_in_argument_list(msg_pairs.len());

        // Update unread conversation count
        self.0.execute(
            &format!(
                r"
WITH msg_labels AS (
    SELECT label_id, COUNT(message_id) AS num_unread
    FROM message_labels WHERE message_id IN ({})
    GROUP BY label_id
)
UPDATE label_conversation_count SET unread=unread+msg_labels.num_unread
FROM msg_labels
WHERE label_conversation_count.label_id = msg_labels.label_id",
                args,
            ),
            params_from_iter(msg_pairs.iter().map(|(id, _)| id)),
        )?;

        // Update conversation
        self.0.execute(
            &format!(
                "UPDATE conversations SET num_unread=num_unread+1 WHERE id IN ({})",
                args
            ),
            params_from_iter(msg_pairs.iter().map(|(_, id)| id)),
        )?;

        // Update conversation labels
        self.0.execute(
            &format!(
                "UPDATE conversation_labels SET ctx_num_unread=ctx_num_unread+1 WHERE conversation_id IN ({})",
                args,
            ),
            params_from_iter(msg_pairs.iter().map(|(_,id)| id)),
        )?;

        // Update message counts
        self.0.execute(
            &format!(
                r"
WITH msg_labels AS (
    SELECT label_id, COUNT(message_id) AS diff_unread
    FROM message_labels
    WHERE message_id IN ({})
    GROUP BY label_id
)

UPDATE label_message_count SET unread=unread+msg_labels.diff_unread
FROM msg_labels
WHERE label_message_count.label_id = msg_labels.label_id
        ",
                args
            ),
            params_from_iter(msg_pairs.iter().map(|(id, _)| id)),
        )?;
        Ok(())
    }

    pub fn label_conversation(
        &mut self,
        label_id: LocalLabelId,
        id: LocalConversationId,
    ) -> DBResult<()> {
        self.label_conversations(label_id, std::iter::once(id))
    }

    pub fn label_conversations(
        &mut self,
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let mut label_msg_stmt = self.0.prepare(
            r"
WITH conv_msgs AS (
    SELECT id,? AS label_id FROM messages WHERE conversation_id=?
)
INSERT OR IGNORE INTO message_labels (message_id, label_id) SELECT * FROM conv_msgs RETURNING message_id
",
        )?;

        for id in ids {
            // Check if conversation already has this label
            let (has_label,is_unread) : (bool,bool) =self.0.query_row("SELECT IFNULL(conversation_id,0), IFNULL(ctx_num_unread,0)<>0 FROM conversation_labels WHERE label_id=? AND conversation_id=?", (label_id,id),|r|{Ok((r.get(0)?,r.get(1)?))}).optional()?.unwrap_or((false,false));

            // label all conversation messages
            let msg_ids: Vec<LocalMessageId> =
                mapped_rows_to_vec(label_msg_stmt.query_map((label_id, id), |r| r.get(0))?)?;

            if !msg_ids.is_empty() {
                // collect info.
                struct MessageInfo {
                    size: u64,
                    time: u64,
                    expiration_time: u64,
                    count: u64,
                    unread: u64,
                    num_attachments: u64,
                    snooze_time: u64,
                }

                let info = self.0.query_row(
                    &format!(
                        r"
SELECT
    MAX(time) AS time,
    MAX(expiration_time) AS expiration_time,
    SUM(size) AS size,
    COUNT(id) AS `count`,
    SUM(unread) AS unread,
    SUM(num_attachments) AS num_attachments,
    MAX(snooze_time) as snooze_time
FROM messages
WHERE id IN ({}) GROUP BY conversation_id",
                        gen_variable_in_argument_list(msg_ids.len())
                    ),
                    params_from_iter(msg_ids),
                    |r| {
                        Ok(MessageInfo {
                            size: r.get(2)?,
                            time: r.get(0)?,
                            expiration_time: r.get(1)?,
                            count: r.get(3)?,
                            unread: r.get(4)?,
                            num_attachments: r.get(5)?,
                            snooze_time: r.get(6)?,
                        })
                    },
                )?;

                // create label if not exists
                // must take into account labels that have already been applied
                self.0.execute(
                    r"
INSERT INTO conversation_labels (
    label_id,
    conversation_id,
    ctx_time,
    ctx_size,
    ctx_num_messages,
    ctx_num_unread,
    ctx_num_attachments,
    ctx_expiration_time,
    ctx_snooze_time
) VALUES (?,?,?,?,?,?,?,?,?)
ON CONFLICT(label_id, conversation_id) DO UPDATE SET
    ctx_time=CASE WHEN excluded.ctx_time > ctx_time THEN excluded.ctx_time ELSE ctx_time END,
    ctx_snooze_time=CASE WHEN excluded.ctx_snooze_time > ctx_snooze_time THEN excluded.ctx_snooze_time ELSE ctx_snooze_time END,
    ctx_expiration_time=CASE WHEN excluded.ctx_expiration_time > ctx_expiration_time THEN excluded.ctx_expiration_time ELSE ctx_expiration_time END,
    ctx_size=ctx_size+excluded.ctx_size,
    ctx_num_messages=ctx_num_messages+excluded.ctx_num_messages,
    ctx_num_unread=ctx_num_unread+excluded.ctx_num_unread,
    ctx_num_attachments=ctx_num_attachments+excluded.ctx_num_attachments
", (label_id, id, info.time, info.size, info.count, info.unread, info.num_attachments, info.expiration_time, info.snooze_time)
                )?;
                // update message counts
                self.0.execute(
                    r"
INSERT INTO label_message_count (label_id, total, unread) VALUES (?,?,?)
ON CONFLICT(label_id) DO UPDATE SET total=total+excluded.total, unread=unread+excluded.unread",
                    (label_id, info.count, info.unread),
                )?;
                // only update conv label count if conv does not  have label or has not been marked unread yet
                let should_inc = !has_label;
                let should_inc_unread = !is_unread && info.unread != 0;
                if should_inc_unread || should_inc {
                    self.0.execute(
                        r"UPDATE label_conversation_count SET unread=unread+?, total=total+? WHERE label_id=?",
                        (should_inc_unread, should_inc, label_id),
                    )?;
                }
            } else {
                // Fallback without message metadata. We should grab the highest time values from
                // all the remaining labels assigned to this conversation. All conversations
                // messages will at the minimum, always assigned the All Mail label.
                self.0.execute(
                    r"
INSERT OR IGNORE INTO conversation_labels
SELECT
    ?1,
    ?2,
    IFNULL(MAX(ctx_time),0) AS time,
    IFNULL(MAX(ctx_size),0) AS size,
    IFNULL(MAX(ctx_num_messages),0) AS num_messages,
    IFNULL(MAX(ctx_num_unread),0) AS num_unread,
    IFNULL(MAX(ctx_num_attachments),0) AS num_attachments,
    IFNULL(MAX(ctx_expiration_time),0) AS expiration_time,
    IFNULL(MAX(ctx_snooze_time),0) AS snooze_time
FROM conversation_labels
WHERE conversation_id=?1
",
                    (id, label_id),
                )?;

                if !has_label {
                    // bump the label count by one
                    self.0.execute(
                        r"
INSERT INTO label_conversation_count (label_id, total, unread) VALUES (?,?,0)
ON CONFLICT(label_id) DO UPDATE SET total=total+excluded.total",
                        (label_id, 1),
                    )?;
                }
            }
        }

        Ok(())
    }
    pub fn unlabel_conversation(
        &mut self,
        label_id: LocalLabelId,
        id: LocalConversationId,
    ) -> DBResult<()> {
        self.unlabel_conversations(label_id, std::iter::once(id))
    }

    pub fn unlabel_conversations(
        &mut self,
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let mut label_msg_stmt = self.0.prepare(
        r"
WITH conv_msgs AS (
    SELECT id, unread FROM messages WHERE conversation_id=?1
)
DELETE FROM message_labels WHERE message_id IN (SELECT id FROM messages WHERE conversation_id=?1) AND message_labels.label_id=?2 RETURNING message_id
",
    )?;

        for id in ids {
            // unlabel all conversation messages.
            let msg_ids: Vec<LocalMessageId> =
                mapped_rows_to_vec(label_msg_stmt.query_map((id, label_id), |r| r.get(0))?)?;

            // can only do this part if we have conv metadata
            if !msg_ids.is_empty() {
                // get unread count
                let num_msg_unread: u64 = self.0.query_row(
                    &format!(
                        "SELECT SUM(unread) FROM messages WHERE id IN ({})",
                        gen_variable_in_argument_list(msg_ids.len())
                    ),
                    params_from_iter(&msg_ids),
                    |r| r.get(0),
                )?;

                // update message counts
                self.0.execute(
                    r"
INSERT INTO label_message_count (label_id, total, unread) VALUES (?,?,?)
ON CONFLICT(label_id) DO UPDATE SET total=total-excluded.total, unread=unread-excluded.unread",
                    (label_id, msg_ids.len(), num_msg_unread),
                )?;
            }
            // Remove label
            let conv_label_unread : Option<u64> = self.0.query_row("DELETE FROM conversation_labels WHERE conversation_id=? AND label_id=? RETURNING ctx_num_unread", (id, label_id), |r| r.get(0)).optional()?;
            // only update conv label count if conv had messages in this label
            if let Some(num_unread) = conv_label_unread {
                self.0.execute(
                    r"UPDATE label_conversation_count SET unread=unread-?, total=total-1 WHERE label_id=?",
                    (num_unread > 0, label_id),
                )?;
            }
        }

        Ok(())
    }
}

const RESOLVE_LABEL_ID_STATEMENT: &str = "SELECT id FROM labels WHERE rid = ?";

struct ConversationSelector {}
impl ConversationSelector {
    fn query() -> &'static str {
        r"WITH json_conversation_labels AS (
    SELECT
        C.conversation_id as cid,
        json_group_array(
            json_object(
                'id', L.id,
                'name', L.name,
                'color', L.color
            )
        ) as labels
    FROM conversation_labels C
    INNER JOIN labels AS L ON C.label_id = L.id AND L.type=1
    GROUP BY C.conversation_id
),
json_conv_attachments AS (
    SELECT
        C.conversation_id as cid,
        json_group_array(
            json_object(
                'id', A.id,
                'rid', A.rid,
                'name', A.name,
                'mime_type', A.mime_type,
                'disposition', A.disposition,
                'size', A.size
            )
        ) as json_attachments
    FROM conversation_attachments as C
    INNER JOIN attachments AS A ON C.attachment_id = A.id
    GROUP BY C.conversation_id
)

SELECT
    C.id,
    C.rid,
    C.`order`,
    C.subject,
    C.senders,
    C.recipients,
    C.num_messages,
    C.num_unread,
    C.num_attachments,
    C.expiration_time,
    C.size,
    IIF(CF.conversation_id IS NULL, 0,1),
    CLJ.labels,
    CA.json_attachments
FROM conversations AS C
LEFT JOIN json_conversation_labels AS CLJ ON CLJ.cid = C.id
LEFT JOIN json_conv_attachments AS CA ON CA.cid = C.id
LEFT JOIN conversation_labels AS CF ON C.id = CF.conversation_id AND CF.label_id = (SELECT id FROM labels WHERE rid='10')
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
        let senders = deserialize_json_from_row::<Vec<MessageAddress>>(r, 4)?;
        let avatar_information = ConversationAvatarInformation::from_message_addresses(&senders);

        Ok({
            LocalConversation {
                id: r.get(0)?,
                remote_id: r.get(1)?,
                order: r.get(2)?,
                subject: r.get(3)?,
                senders,
                recipients: deserialize_json_from_row::<Vec<MessageAddress>>(r, 5)?,
                num_messages: r.get(6)?,
                num_messages_ctx: 0,
                num_unread: r.get(7)?,
                num_attachments: r.get(8)?,
                expiration_time: r.get(9)?,
                size: r.get(10)?,
                starred: r.get(11)?,
                labels: deserialize_optional_json_from_row::<Vec<LocalInlineLabelInfo>>(r, 12)?,
                time: 0,
                snooze_time: 0,
                attachments: deserialize_optional_json_from_row::<Vec<LocalAttachmentMetadata>>(
                    r, 13,
                )?,
                avatar_information,
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
    SELECT
        C.conversation_id as cid,
        json_group_array(
            json_object(
                'id', L.id,
                'name', L.name,
                'color', L.color
            )
        ) as labels
    FROM conversation_labels C
    INNER JOIN labels AS L ON C.label_id = L.id AND L.type=1
    GROUP BY C.conversation_id
),
json_conv_attachments AS (
    SELECT
        C.conversation_id as cid,
        json_group_array(
            json_object(
                'id', A.id,
                'rid', A.rid,
                'name', A.name,
                'mime_type', A.mime_type,
                'disposition', A.disposition,
                'size', A.size
            )
        ) as json_attachments
    FROM conversation_attachments as C
    INNER JOIN attachments AS A ON C.attachment_id = A.id
    GROUP BY C.conversation_id
)

SELECT
    C.id,
    C.rid,
    C.`order`,
    C.subject,
    C.senders,
    C.recipients,
    C.expiration_time,
    ifnull(CL.ctx_time,0),
    ifnull(CL.ctx_size,0),
    ifnull(CL.ctx_num_messages,0),
    ifnull(CL.ctx_num_unread,0),
    ifnull(CL.ctx_num_attachments,0),
    IIF(CF.conversation_id IS NULL, 0,1),
    CLJ.labels,
    CA.json_attachments,
    C.num_messages,
    ifnull(CL.ctx_expiration_time,0),
    ifnull(CL.ctx_snooze_time,0)
FROM conversations AS C
INNER JOIN conversation_labels AS CL ON CL.conversation_id=C.id AND CL.label_id=?
LEFT JOIN json_conversation_labels AS CLJ ON CLJ.cid = C.id
LEFT JOIN json_conv_attachments AS CA ON CA.cid = C.id
LEFT JOIN conversation_labels AS CF ON C.id = CF.conversation_id AND CF.label_id = (SELECT id FROM labels WHERE rid='10')
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
        let senders = deserialize_json_from_row::<Vec<MessageAddress>>(r, 4)?;
        let avatar_information = ConversationAvatarInformation::from_message_addresses(&senders);

        Ok(LocalConversation {
            id: r.get(0)?,
            remote_id: r.get(1)?,
            order: r.get(2)?,
            subject: r.get(3)?,
            senders,
            recipients: deserialize_json_from_row::<Vec<MessageAddress>>(r, 5)?,
            time: r.get(7)?,
            size: r.get(8)?,
            num_messages_ctx: r.get(9)?,
            num_unread: r.get(10)?,
            num_attachments: r.get(11)?,
            starred: r.get(12)?,
            labels: deserialize_optional_json_from_row::<Vec<LocalInlineLabelInfo>>(r, 13)?,
            attachments: deserialize_optional_json_from_row::<Vec<LocalAttachmentMetadata>>(r, 14)?,
            num_messages: r.get(15)?,
            expiration_time: r.get(16)?,
            snooze_time: r.get(17)?,
            avatar_information,
        })
    }
}
