use crate::avatar::AvatarInformation;
use crate::db::json::{
    deserialize_json_from_row, deserialize_optional_json_from_row, JsonWriteBuffer,
};
use crate::db::{
    DBResult, LocalAttachmentMetadata, LocalAttachmentMetadataSelector, LocalConversationId,
    LocalInlineLabelInfo, LocalLabelId, LocalMessageBodyMetadata, LocalMessageCount,
    LocalMessageId, LocalMessageMetadata, MailSqliteConnectionImpl,
};
use indoc::indoc;
use proton_api_mail::domain::{Message, MessageAddress, MessageCount, MessageId, MessageMetadata};
use proton_sqlite3::rusqlite::{params_from_iter, OptionalExtension, Row, Statement};
use proton_sqlite3::utils::{
    gen_variable_in_argument_list, mapped_rows_into_vec, mapped_rows_to_vec, StmtIndexAllocator,
};
use std::collections::BTreeSet;

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
        let mut attachment_stmt = self.create_message_attachment_ref_statement()?;

        let mut gen_unknown_conversation_stmt = self.0.prepare(indoc! {
            "INSERT INTO conversations(
                rid,
                `order`,
                subject,
                senders,
                recipients,
                num_messages,
                num_unread,
                num_attachments,
                expiration_time,
                size,
                is_known
            ) VALUES (?,0,'','','', 0, 0, 0,0,0,0)
            ON CONFLICT(rid) DO NOTHING
            "
        })?;

        for metadata in metadata {
            // Create a dummy conversation if it has not been synced before, so we can resolve
            // to a local id that is valid. Retrieving the conversation from remote or events
            // will initialize all the missing fields.
            gen_unknown_conversation_stmt.execute([&metadata.conversation_id])?;

            bind_message_metadata_create(
                &mut msg_stmt,
                metadata,
                &mut to_list_buffer,
                &mut cc_list_buffer,
                &mut bcc_list_buffer,
            )?;
            let local_id: LocalMessageId = msg_stmt
                .raw_query()
                .next()?
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
                let attachment_id =
                    attachment_stmt.insert(Some(&metadata.address_id), attachment, local_id)?;
                message_to_attachment_stmt.execute((local_id, attachment_id))?;
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
        if ids.len() == 0 {
            return Ok(Vec::new());
        }
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

    /// Get message metadata for a conversation with `id`.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn messages_metadata_for_conversation(
        &self,
        id: LocalConversationId,
    ) -> DBResult<Vec<LocalMessageMetadata>> {
        let mut stmt = self
            .0
            .prepare(&LocalMessageMetadataSelector::query_with_conversation_id())?;
        let r = stmt.query_map([id], LocalMessageMetadataSelector::from_row)?;
        let result = mapped_rows_to_vec(r)?;
        // If there are no messages for this conversation it means the conversation
        // has been removed. There should always be one message.
        if result.is_empty() {
            return Err(proton_sqlite3::rusqlite::Error::QueryReturnedNoRows);
        }

        Ok(result)
    }

    /// Get up to `count` message metadata for all messages in `label_id`.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn message_metadata_list(
        &self,
        label_id: LocalLabelId,
        count: usize,
    ) -> DBResult<Vec<LocalMessageMetadata>> {
        let mut stmt = self
            .0
            .prepare(&LocalMessageMetadataSelector::query_with_label_and_limit())?;
        let r = stmt.query_map((label_id, count), LocalMessageMetadataSelector::from_row)?;
        mapped_rows_to_vec(r)
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
            "INSERT INTO label_message_count VALUES \
        ((SELECT id FROM labels WHERE rid=?),?,?) ON CONFLICT (label_id) DO UPDATE SET \
        total=excluded.total, unread=excluded.unread",
        )?;

        for count in counts {
            stmt.execute((&count.label_id, count.total, count.unread))?;
        }
        Ok(())
    }

    /// Get all message counts.
    ///
    /// # Errors
    /// Return error if the query fails.
    pub fn message_counts(&self) -> DBResult<Vec<LocalMessageCount>> {
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

    /// Get message counts for label with `id`.
    ///
    /// # Errors
    /// Returns error if the query fails.
    pub fn message_count_for_label(&self, id: LocalLabelId) -> DBResult<Option<LocalMessageCount>> {
        self.0
            .query_row(
                "SELECT * FROM label_message_count WHERE label_id = ?",
                [id],
                |r| {
                    Ok(LocalMessageCount {
                        id: r.get(0)?,
                        total: r.get(1)?,
                        unread: r.get(2)?,
                    })
                },
            )
            .optional()
    }

    pub fn delete_remote_message(&mut self, id: &MessageId) -> DBResult<()> {
        self.0.execute("DELETE FROM messages WHERE rid=?", [id])?;
        Ok(())
    }

    /// Get the attachments of a message.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn message_attachments_metadata(
        &self,
        id: LocalMessageId,
    ) -> DBResult<Option<Vec<LocalAttachmentMetadata>>> {
        let query = format!(
            r"{}
JOIN message_attachments ON att.id=message_attachments.attachment_id AND
    message_attachments.message_id=?
",
            LocalAttachmentMetadataSelector::query(),
        );
        let mut stmt = self.0.prepare(&query)?;
        let rows = stmt.query_map([id], LocalAttachmentMetadataSelector::from_row)?;
        let r = mapped_rows_to_vec(rows)?;
        if r.is_empty() {
            return Ok(None);
        }
        Ok(Some(r))
    }

    pub fn mark_local_message_as_deleted(&mut self, id: LocalMessageId) -> DBResult<()> {
        self.mark_local_messages_as_deleted(std::iter::once(id))
    }

    pub fn mark_local_messages_as_deleted(
        &mut self,
        ids: impl ExactSizeIterator<Item = LocalMessageId>,
    ) -> DBResult<()> {
        let args = gen_variable_in_argument_list(ids.len());

        // mark messages as deleted -> conversation ids (?)
        let mut stmt = self.0.prepare(&format!(
            "UPDATE messages SET deleted=1 WHERE id IN ({}) AND deleted=0 RETURNING id",
            args
        ))?;

        // Only run the logic on messages that were not already deleted.
        let ids = mapped_rows_to_vec(stmt.query_map(params_from_iter(ids), |r| r.get(0))?)?;

        // Update conversation counters
        self.remove_messages_from_conversation_labels(&ids)?;

        // Update message counter
        self.update_message_counters_after_delete_local(&ids)?;
        Ok(())
    }

    pub fn unmark_local_message_as_deleted(&mut self, id: LocalMessageId) -> DBResult<()> {
        self.unmark_local_messages_as_deleted(std::iter::once(id))
    }
    pub fn unmark_local_messages_as_deleted(
        &mut self,
        ids: impl ExactSizeIterator<Item = LocalMessageId>,
    ) -> DBResult<()> {
        let args = gen_variable_in_argument_list(ids.len());

        // mark messages as deleted -> conversation ids (?)
        let mut stmt = self.0.prepare(&format!(
            "UPDATE messages SET deleted=0 WHERE id IN ({}) AND deleted=1 RETURNING id",
            args
        ))?;

        // Only run the logic on messages that were not already deleted.
        let ids = mapped_rows_to_vec(stmt.query_map(params_from_iter(ids), |r| r.get(0))?)?;

        // Update conversation counters
        self.add_messages_to_conversation_labels(&ids)?;

        // Update message counter
        self.update_message_counters_after_undelete_local(&ids)?;
        Ok(())
    }

    fn remove_messages_from_conversation_labels(&mut self, ids: &[LocalMessageId]) -> DBResult<()> {
        self.remove_or_add_messages_from_conversation_labels(ids, true)
    }

    fn add_messages_to_conversation_labels(&mut self, ids: &[LocalMessageId]) -> DBResult<()> {
        self.remove_or_add_messages_from_conversation_labels(ids, false)
    }

    fn remove_or_add_messages_from_conversation_labels(
        &mut self,
        ids: &[LocalMessageId],
        delete: bool,
    ) -> DBResult<()> {
        let message_id_args = gen_variable_in_argument_list(ids.len());

        let arithmetic = if delete { '-' } else { '+' };
        let query = format!(
            r"
WITH
deleted_messages AS (
    SELECT
        id,
        conversation_id,
        size,
        unread,
        num_attachments
    FROM messages
    WHERE messages.id IN ({message_id_args})
),
deleted_message_labels AS (
    SELECT
        message_labels.label_id,
        message_labels.message_id
    FROM message_labels
    JOIN deleted_messages ON message_id=deleted_messages.id
),
conv_messages AS (
    SELECT
        l.label_id,
        MAX(m.time) as time,
        MAX(m.expiration_time) as expiration_time,
        MAX(m.snooze_time) as snooze_time
    FROM messages AS m JOIN deleted_messages AS dm ON dm.conversation_id=m.conversation_id
    JOIN message_labels AS l ON l.message_id=m.id
    WHERE m.deleted=0
    GROUP BY l.label_id
),
label_modifiers AS (
    SELECT
       cm.conversation_id,
       ml.label_id,
       COUNT(cm.id) as num_messages,
       SUM(cm.unread) as num_unread,
       IFNULL(mt.time,0) as time,
       SUM(cm.num_attachments) as num_attachments,
       SUM(size) as size,
       IFNULL(MAX(mt.expiration_time),0) as expiration_time,
       IFNULL(MAX(mt.snooze_time),0) as snooze_time
    FROM deleted_messages AS cm
    JOIN deleted_message_labels AS ml ON cm.id = ml.message_id
    LEFT JOIN conv_messages AS mt on mt.label_id = ml.label_id
    GROUP BY cm.conversation_id, ml.label_id
)
UPDATE conversation_labels SET
   ctx_num_attachments=ctx_num_attachments{arithmetic}label_modifiers.num_attachments,
   ctx_num_messages=ctx_num_messages{arithmetic}label_modifiers.num_messages,
   ctx_num_unread=ctx_num_unread{arithmetic}label_modifiers.num_unread,
   ctx_time=label_modifiers.time,
   ctx_expiration_time=label_modifiers.expiration_time,
   ctx_snooze_time=label_modifiers.snooze_time,
   ctx_size=ctx_size{arithmetic}label_modifiers.size
FROM label_modifiers WHERE
    label_modifiers.conversation_id=conversation_labels.conversation_id AND
    conversation_labels.label_id=label_modifiers.label_id
RETURNING label_id"
        );

        // Execute update and get updates
        let label_ids: BTreeSet<LocalLabelId> = {
            let mut label_ids = BTreeSet::new();
            let mut stmt = self.0.prepare(&query)?;
            let rows = stmt.query_map(params_from_iter(ids), |r| r.get(0))?;
            for row in rows {
                let label_id = row?;
                label_ids.insert(label_id);
            }
            label_ids
        };

        // Recalculate label conversation unread count.
        let query = format!(
            r"
UPDATE label_conversation_count SET
    unread=delta.num_unread,
    total=delta.num_messages
FROM(
    SELECT
        cl.label_id,
        SUM(cl.ctx_num_messages <> 0) AS num_messages,
        SUM(cl.ctx_num_unread <> 0) AS num_unread
    FROM  conversation_labels AS cl
    WHERE cl.label_id IN ({})
    GROUP BY cl.label_id
) AS delta
WHERE label_conversation_count.label_id=delta.label_id
            ",
            gen_variable_in_argument_list(label_ids.len())
        );
        self.0.execute(&query, params_from_iter(label_ids.iter()))?;

        // Update conversation non-context count
        let query = format!(
            r"
UPDATE conversations SET
    num_messages=num_messages{arithmetic}deltas.count_delta,
    num_unread=num_unread{arithmetic}deltas.unread_delta
FROM(
    SELECT
        conversation_id,
        COUNT(id) As count_delta,
        SUM(unread) AS unread_delta
    FROM messages WHERE id IN ({message_id_args})
    GROUP BY conversation_id
) AS deltas
WHERE deltas.conversation_id=conversations.id
"
        );
        self.0.execute(&query, params_from_iter(ids))?;

        if delete {
            // if conversation has no messages, mark it as deleted
            self.0.execute(
                "UPDATE conversations SET deleted=1 WHERE num_messages=0 AND deleted=0",
                (),
            )?;
        } else {
            // if conversation has messages, mark undelete it if it was deleted
            self.0.execute(
                "UPDATE conversations SET deleted=0 WHERE num_messages<>0 AND deleted=1",
                (),
            )?;
        }
        Ok(())
    }

    pub(super) fn mark_local_messages_as_deleted_with_conversation_ids(
        &mut self,
        label_id: LocalLabelId,
        conversation_ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let args = gen_variable_in_argument_list(conversation_ids.len());
        let mut msg_ids: Vec<LocalMessageId> = Vec::with_capacity(conversation_ids.len());
        let mut update_stmt = self.0.prepare(&format!(
            r"UPDATE messages SET deleted=1
             WHERE
                conversation_id IN ({})
                AND deleted = 0
                AND id IN (SELECT message_id FROM message_labels WHERE label_id=?)
            RETURNING id",
            args
        ))?;

        let mut alloc = StmtIndexAllocator::new();
        for id in conversation_ids {
            update_stmt.raw_bind_parameter(alloc.fetch_and_add(), id)?;
        }
        update_stmt.raw_bind_parameter(alloc.fetch_and_add(), label_id)?;

        mapped_rows_into_vec(&mut msg_ids, update_stmt.raw_query().mapped(|r| r.get(0)))?;

        if msg_ids.is_empty() {
            return Ok(());
        }

        self.update_message_counters_after_delete_local(&msg_ids)?;
        Ok(())
    }

    pub(super) fn unmark_local_messages_as_deleted_with_conversation_ids(
        &mut self,
        label_id: LocalLabelId,
        conversation_ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let args = gen_variable_in_argument_list(conversation_ids.len());
        let mut msg_ids: Vec<LocalMessageId> = Vec::with_capacity(conversation_ids.len());
        let mut update_stmt = self.0.prepare(&format!(
            r"UPDATE messages SET deleted=0
             WHERE
                conversation_id IN ({})
                AND deleted = 1
                AND id IN (SELECT message_id FROM message_labels WHERE label_id=?)
            RETURNING id",
            args
        ))?;

        let mut alloc = StmtIndexAllocator::new();
        for id in conversation_ids {
            update_stmt.raw_bind_parameter(alloc.fetch_and_add(), id)?;
        }
        update_stmt.raw_bind_parameter(alloc.fetch_and_add(), label_id)?;

        mapped_rows_into_vec(&mut msg_ids, update_stmt.raw_query().mapped(|r| r.get(0)))?;

        if msg_ids.is_empty() {
            return Ok(());
        }

        self.update_message_counters_after_undelete_local(&msg_ids)?;
        Ok(())
    }

    pub(super) fn mark_local_messages_as_deleted_with_conversation_ids_all_mail(
        &mut self,
        conversation_ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let args = gen_variable_in_argument_list(conversation_ids.len());
        let mut msg_ids: Vec<LocalMessageId> = Vec::with_capacity(conversation_ids.len());
        let mut update_stmt = self.0.prepare(&format!(
            "UPDATE messages SET deleted=1 WHERE conversation_id IN ({}) AND deleted = 0 RETURNING id",
            args
        ))?;
        mapped_rows_into_vec(
            &mut msg_ids,
            update_stmt.query_map(params_from_iter(conversation_ids), |r| r.get(0))?,
        )?;

        if msg_ids.is_empty() {
            return Ok(());
        }

        self.update_message_counters_after_delete_local(&msg_ids)?;
        Ok(())
    }

    pub(super) fn unmark_local_messages_as_deleted_with_conversation_ids_all_mail(
        &mut self,
        conversation_ids: impl ExactSizeIterator<Item = LocalConversationId>,
    ) -> DBResult<()> {
        let args = gen_variable_in_argument_list(conversation_ids.len());
        let mut msg_ids: Vec<LocalMessageId> = Vec::with_capacity(conversation_ids.len());
        let mut update_stmt = self.0.prepare(&format!(
            "UPDATE messages SET deleted=0 WHERE conversation_id IN ({}) AND deleted = 1 RETURNING id",
            args
        ))?;
        mapped_rows_into_vec(
            &mut msg_ids,
            update_stmt.query_map(params_from_iter(conversation_ids), |r| r.get(0))?,
        )?;

        if msg_ids.is_empty() {
            return Ok(());
        }

        self.update_message_counters_after_undelete_local(&msg_ids)?;
        Ok(())
    }

    fn update_message_counters_after_delete_local(
        &mut self,
        ids: &[LocalMessageId],
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare(&format!(
            r"UPDATE label_message_count AS lmc SET
    total=total-dm.num_messages,
    unread=unread-dm.num_unread
FROM (
    SELECT
        ml.label_id,
        SUM(m.unread) AS `num_unread`,
        COUNT(m.id) AS `num_messages`
    FROM messages AS m
    JOIN message_labels AS ml ON ml.message_id = m.id
    WHERE m.id IN ({})
    GROUP BY ml.label_id
) AS dm
WHERE lmc.label_id = dm.label_id
",
            gen_variable_in_argument_list(ids.len())
        ))?;
        stmt.execute(params_from_iter(ids))?;
        Ok(())
    }

    fn update_message_counters_after_undelete_local(
        &mut self,
        ids: &[LocalMessageId],
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare(&format!(
            r"UPDATE label_message_count AS lmc SET
    total=total+dm.num_messages,
    unread=unread+dm.num_unread
    FROM (
SELECT
    ml.label_id,
    SUM(m.unread) AS `num_unread`,
    COUNT(m.id) AS `num_messages`
    FROM messages AS m
    JOIN message_labels AS ml ON ml.message_id = m.id
    WHERE m.id IN ({})
    GROUP BY ml.label_id
) AS dm
WHERE lmc.label_id = dm.label_id
",
            gen_variable_in_argument_list(ids.len())
        ))?;
        stmt.execute(params_from_iter(ids))?;
        Ok(())
    }

    /// Create or update message bodies.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn create_or_update_message_body(
        &mut self,
        message: &Message,
    ) -> DBResult<LocalMessageBodyMetadata> {
        // Create message body
        let query = indoc! {"
            INSERT INTO message_bodies (
               id,
               header,
               parsed_headers,
               mime_type
            ) VALUES ((SELECT id FROM messages WHERE rid=?),?,?,?)
            ON CONFLICT(id) DO UPDATE SET
                header=excluded.header,
                parsed_headers=excluded.parsed_headers,
                mime_type=excluded.mime_type
            RETURNING id
        "};

        let mut json_writer = JsonWriteBuffer::new();
        let parsed_headers = json_writer.serialize(&message.parsed_headers)?;
        let local_id: LocalMessageId = self.0.query_row(
            query,
            (
                &message.metadata.id,
                &message.header,
                &parsed_headers,
                &message.mime_type,
            ),
            |r| r.get(0),
        )?;

        // Update attachment headers
        if !message.attachments.is_empty() {
            let conversation_id = self.message_conversation_id(local_id)?;
            self.create_or_update_attachments_from_message(
                local_id,
                conversation_id,
                &message.attachments,
            )?;
        }

        Ok(LocalMessageBodyMetadata {
            id: local_id,
            rid: Some(message.metadata.id.clone()),
            header: message.header.clone(),
            parsed_headers: message.parsed_headers.clone(),
            mime_type: message.mime_type,
            address_id: message.metadata.address_id.clone(),
        })
    }

    /// Retrieve the conversation id for message with `id`
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn message_conversation_id(
        &self,
        id: LocalMessageId,
    ) -> DBResult<Option<LocalConversationId>> {
        self.0.query_row(
            "SELECT conversation_id FROM messages WHERE id=?",
            [id],
            |r| r.get(0),
        )
    }

    /// Retrieve the remote id for a message with `id`
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn message_remote_id(&self, id: LocalMessageId) -> DBResult<Option<Option<MessageId>>> {
        self.0
            .query_row("SELECT rid FROM messages WHERE id=?", [id], |r| r.get(0))
            .optional()
    }

    /// Retrieve the local id for a message with `remote_id`
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn message_id_from_remote_id(
        &self,
        remote_id: &MessageId,
    ) -> DBResult<Option<LocalMessageId>> {
        self.0
            .query_row("SELECT id FROM messages WHERE rid=?", [remote_id], |r| {
                r.get(0)
            })
            .optional()
    }

    /// Get the message body for a message with `id`.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn message_body(&self, id: LocalMessageId) -> DBResult<Option<LocalMessageBodyMetadata>> {
        let query = indoc! {"
             SELECT
                MB.id,
                M.rid,
                MB.header,
                MB.parsed_headers,
                MB.mime_type,
                M.address_id
             FROM message_bodies AS MB
             JOIN messages AS M ON M.id=MB.id
             WHERE MB.id =?
        "};

        self.0
            .query_row(query, [id], |r| {
                Ok(LocalMessageBodyMetadata {
                    id: r.get(0)?,
                    rid: r.get(1)?,
                    header: r.get(2)?,
                    parsed_headers: deserialize_json_from_row(r, 3)?,
                    mime_type: r.get(4)?,
                    address_id: r.get(5)?,
                })
            })
            .optional()
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
    conversation_id,
    rid,
    address_id,
    `order`,
    subject,
    unread,
    sender_address,
    sender_name,
    sender_is_proton,
    sender_is_simple_login,
    sender_bimi_selector,
    sender_display_image,
    to_list,
    cc_list,
    bcc_list,
    time,
    size,
    expiration_time,
    is_replied,
    is_replied_all,
    is_forwarded,
    external_id,
    num_attachments,
    flags,
    snooze_time
) VALUES ((SELECT id FROM conversations WHERE rid=?),{})
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
    snooze_time=excluded.snooze_time
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
        m.snooze_time,
    }

    Ok(())
}

struct LocalMessageMetadataSelector {}
const MESSAGE_SELECTOR_ORDER_CLAUSE: &str = " GROUP BY M.id ORDER BY M.time DESC, M.`order` DESC ";
const MESSAGE_SELECTOR_ORDER_CLAUSE_CONVERSATION: &str =
    " GROUP BY M.id ORDER BY M.time ASC, M.`order` DESC ";

const MESSAGE_SELECTOR_QUERY_BEGIN: &str = r"
WITH json_message_labels AS (
    SELECT
    C.message_id as cid,
    json_group_array(
        json_object(
            'id', L.id,
            'name', L.name,
            'color', L.color
        )
    ) as labels
    FROM message_labels C
    INNER JOIN labels AS L ON C.label_id = L.id AND L.type=1
GROUP BY C.message_id
),
json_message_attachments AS (
    SELECT
    C.message_id as cid,
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
    FROM message_attachments as C
    INNER JOIN attachments AS A ON C.attachment_id = A.id
    GROUP BY C.message_id
)
SELECT
    id,
    rid,
    address_id,
    conversation_id,
    `order`,
    subject,
    unread,
    sender_address,
    sender_name,
    sender_is_proton,
    sender_is_simple_login,
    sender_bimi_selector,
    sender_display_image,
    to_list,
    cc_list,
    bcc_list,
    time,
    size,
    expiration_time,
    is_replied,
    is_replied_all,
    is_forwarded,
    external_id,
    num_attachments,
    flags,
    IIF(ml.message_id IS NULL, 0,1),
    snooze_time,
    MA.json_attachments,
    MLJ.labels
FROM messages AS M
LEFT JOIN json_message_attachments AS MA ON MA.cid = M.id
LEFT JOIN message_labels AS ml ON M.id = ml.message_id AND ml.label_id = (SELECT id FROM labels WHERE rid='10')
LEFT JOIN json_message_labels AS MLJ ON MLJ.cid = M.id
";

const MESSAGE_SELECTOR_QUERY_END: &str = r"
WHERE deleted=0
";

impl LocalMessageMetadataSelector {
    fn query() -> String {
        format!("{MESSAGE_SELECTOR_QUERY_BEGIN}{MESSAGE_SELECTOR_QUERY_END}")
    }

    fn query_with_id() -> String {
        format!("{} AND id=?", Self::query())
    }

    fn query_with_conversation_id() -> String {
        format!(
            "{} AND conversation_id=? {MESSAGE_SELECTOR_ORDER_CLAUSE_CONVERSATION}",
            Self::query()
        )
    }

    fn query_with_id_in(count: usize) -> String {
        format!(
            "{} AND id IN ({})",
            Self::query(),
            gen_variable_in_argument_list(count)
        )
    }
    fn query_with_label_and_limit() -> String {
        const LABEL_CLAUSE: &str =
            "INNER JOIN message_labels AS MSL ON MSL.message_id=M.id AND MSL.label_id=?";
        format!("{MESSAGE_SELECTOR_QUERY_BEGIN} {LABEL_CLAUSE} {MESSAGE_SELECTOR_QUERY_END} {MESSAGE_SELECTOR_ORDER_CLAUSE} LIMIT ?")
    }

    fn from_row(r: &Row) -> DBResult<LocalMessageMetadata> {
        let sender = MessageAddress {
            address: r.get(7)?,
            name: r.get(8)?,
            is_proton: r.get(9)?,
            is_simple_login: r.get(10)?,
            bimi_selector: r.get(11)?,
            display_sender_image: r.get(12)?,
        };

        let avatar_information = AvatarInformation::from_message_address(&sender);

        Ok(LocalMessageMetadata {
            id: r.get(0)?,
            rid: r.get(1)?,
            address_id: r.get(2)?,
            conversation_id: r.get(3)?,
            order: r.get(4)?,
            subject: r.get(5)?,
            unread: r.get(6)?,
            sender,
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
            snooze_time: r.get(26)?,
            attachments: deserialize_optional_json_from_row::<Vec<LocalAttachmentMetadata>>(r, 27)?,
            labels: deserialize_optional_json_from_row::<Vec<LocalInlineLabelInfo>>(r, 28)?,
            avatar_information,
        })
    }
}
