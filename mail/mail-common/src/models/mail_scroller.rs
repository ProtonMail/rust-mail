use crate::datatypes::ContextualConversation;
use crate::models::Conversation;
use indoc::formatdoc;
use proton_core_common::datatypes::{LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use proton_sqlite3::rusqlite::types::{FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use stash::exports::{FromSql, ToSql, Value};
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};

/// Conversation and message read filter.
#[derive(Debug, Default, Clone, PartialEq, Hash, Eq, Copy)]
#[repr(u8)]
pub enum ReadFilter {
    /// Return all messages/conversations.
    #[default]
    All = 0,
    /// Return only unread messages/conversations.
    Unread = 1,
    /// Return only read messages/conversations.
    Read = 2,
}

impl From<Option<bool>> for ReadFilter {
    fn from(value: Option<bool>) -> Self {
        match value {
            Some(unread) => {
                if unread {
                    Self::Unread
                } else {
                    Self::Read
                }
            }
            None => Self::All,
        }
    }
}
impl From<ReadFilter> for Option<bool> {
    fn from(value: ReadFilter) -> Self {
        match value {
            ReadFilter::All => None,
            ReadFilter::Unread => Some(true),
            ReadFilter::Read => Some(false),
        }
    }
}

impl ToSql for ReadFilter {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for ReadFilter {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_i64()? {
            0 => Ok(ReadFilter::All),
            1 => Ok(ReadFilter::Unread),
            2 => Ok(ReadFilter::Read),
            v => Err(FromSqlError::OutOfRange(v)),
        }
    }
}

#[derive(Debug, Model, Eq, PartialEq, Clone)]
#[TableName("mail_message_scroll_data")]
pub struct MessageScrollData {
    /// Label id used in the sync.
    #[IdField]
    pub local_label_id: LocalId,
    /// Read filter used in the sync.
    #[DbField]
    pub unread: ReadFilter,
    /// Last synced message id.
    #[DbField]
    pub remote_message_id: RemoteId,
    /// Last synced message time.
    #[DbField]
    pub message_time: u64,
    /// Last synced message display order.
    #[DbField]
    pub display_order: u64,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

#[derive(Debug, Model, Eq, PartialEq, Clone)]
#[TableName("mail_conversation_scroll_data")]
pub struct ConversationScrollData {
    /// Label id used in the sync.
    #[IdField]
    pub local_label_id: LocalId,
    /// Read filter used in the sync.
    #[DbField]
    pub unread: ReadFilter,
    /// Id of the last synced conversation.
    #[DbField]
    pub remote_conversation_id: RemoteId,
    /// Time of the last synced conversation.
    ///
    /// Note: for filtered conversation (`ReadFilter != ReadFilter::All`) we
    /// need to store the `Conversation.context_time` rather than
    /// `Conversation.Labels[active_label].context_time`
    #[DbField]
    pub conversation_time: u64,
    /// Display order of the last conversation.
    #[DbField]
    pub display_order: u64,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl ConversationScrollData {
    pub async fn find_with_key(
        local_label_id: LocalId,
        unread: ReadFilter,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        Self::find_first(
            "WHERE local_label_id=? AND unread=?",
            params![local_label_id, unread],
            tether,
        )
        .await
    }

    pub async fn save(&mut self, tx: &Bond<'_>) -> Result<(), StashError> {
        // NOTE: save should always update existing records.
        if let Some(existing) = Self::find_with_key(self.local_label_id, self.unread, tx).await? {
            self.row_id = existing.row_id;
        }
        <Self as Model>::save(self, tx).await
    }

    /// Returns the newest element in the synced data.
    pub async fn newest_element(
        &self,
        tether: &Tether,
    ) -> Result<Option<ContextualConversation>, StashError> {
        // NOTE: this is currently unused but can later be used to query
        // the server for new elements before the latest elements.
        let query = self.query(Some(1), true);
        let Some(conv) = Conversation::find_first(
            query,
            params![
                self.local_label_id,
                self.conversation_time,
                self.display_order
            ],
            tether,
        )
        .await?
        else {
            return Ok(None);
        };

        assert!(conv.remote_id.is_some());
        Ok(ContextualConversation::new(conv, self.local_label_id))
    }

    /// Same as [`visible_elements`] but returns only the number of items that match.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn visible_element_count(&self, tether: &Tether) -> Result<u64, StashError> {
        let query = self.query(None, false);
        Conversation::count(
            query,
            //TODO: this could potentially be abstracted into a function.
            params![
                self.local_label_id,
                self.conversation_time,
                self.display_order
            ],
            tether,
        )
        .await
    }

    /// Return all elements that are in the range of data we have synced from the server.
    ///
    /// This means all elements that a newer than the time and display order of the
    /// last synced element from the server. Elements that are older should not be
    /// displayed.
    ///
    /// It is possible those old elements become available due to interactions of actions
    /// and the event loop, but those should not be displayed until the user scrolls
    /// far enough.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn visible_elements(
        &self,
        tether: &Tether,
    ) -> Result<Vec<ContextualConversation>, StashError> {
        let query = self.query(None, false);
        Ok(Conversation::find(
            query,
            params![
                self.local_label_id,
                self.conversation_time,
                self.display_order
            ],
            tether,
        )
        .await?
        .into_iter()
        .filter_map(|c| ContextualConversation::new(c, self.local_label_id))
        .collect())
    }

    fn query(&self, limit: Option<usize>, require_remote_id: bool) -> String {
        //NOTE: we only check the display order for elements with matching time
        // or we will get incorrect query results.
        let mut query = formatdoc!(
            "
            JOIN conversation_labels
                ON conversations.local_id = conversation_labels.local_conversation_id
            WHERE
                conversation_labels.local_label_id = ?1
            AND
                conversation_labels.deleted = 0
            AND (
                    conversation_labels.context_time > ?2
                OR
                    (conversation_labels.context_time =?2 AND conversations.display_order >= ?3)
                )
            "
        );
        if require_remote_id {
            query += " AND conversations.remote_id <> NULL"
        }

        match self.unread {
            ReadFilter::All => {}
            ReadFilter::Unread => {
                query += " AND conversation_labels.context_num_unread > 0 ";
            }
            ReadFilter::Read => {
                query += " AND conversation_labels.context_num_unread = 0 ";
            }
        }

        query += " ORDER BY
            conversation_labels.context_time DESC,
            conversations.display_order DESC
        ";

        if let Some(limit) = limit {
            query += &format!("LIMIT {limit}");
        }

        query
    }
}
