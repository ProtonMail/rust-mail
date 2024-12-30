use crate::datatypes::{ContextualConversation, ReadFilter};
use crate::models::Conversation;
use indoc::formatdoc;
use proton_core_common::datatypes::{LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use std::ops::Deref;
use typed_builder::TypedBuilder;

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

#[derive(Debug)]
pub struct CachedConverstationScrollData {
    local_label_id: LocalId,
    unread: ReadFilter,
    page_size: usize,
    data: ConversationScrollData,
    cursor: ConversationScrollData,
}

impl CachedConverstationScrollData {
    pub async fn new(
        local_label_id: LocalId,
        unread: ReadFilter,
        page_size: usize,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        let data = ConversationScrollData::find_with_key(local_label_id, unread, tether).await?;

        Ok(match data {
            Some(data) => {
                let data_count = data.visible_element_count(tether).await?;
                let cursor = if data_count > page_size as u64 {
                    // Load first page, could be improved to load only last element but
                    // there is tiny risk that background task could be invoked between
                    // count & page_load which would invalidate the cursor.
                    // so safer option is to load more items to make sure we have reference point
                    let mut items = data
                        .visible_elements_limit(Some(page_size), None, tether)
                        .await?;

                    match items.pop() {
                        Some(last) => ConversationScrollData::builder()
                            .local_label_id(local_label_id)
                            .unread(unread)
                            .remote_conversation_id(last.remote_id.clone().unwrap())
                            .conversation_time(last.time)
                            .display_order(last.display_order)
                            .build(),
                        None => data.clone(),
                    }
                } else {
                    data.clone()
                };

                Some(Self {
                    local_label_id,
                    unread,
                    page_size,
                    data,
                    cursor,
                })
            }
            None => None,
        })
    }

    pub async fn fetch_more(
        &mut self,
        tether: &Tether,
    ) -> Result<Vec<ContextualConversation>, StashError> {
        let all = self.data.visible_element_count(tether).await?;
        let cursor_count = self.cursor.visible_element_count(tether).await?;

        if cursor_count < all {
            let offset = Some(cursor_count);
            let limit = Some(self.page_size);
            let items = self
                .data
                .visible_elements_limit(limit, offset, tether)
                .await?;
            let cursor = match items.last() {
                Some(last) => ConversationScrollData::builder()
                    .local_label_id(self.local_label_id)
                    .unread(self.unread)
                    .remote_conversation_id(last.remote_id.clone().unwrap())
                    .conversation_time(last.time)
                    .display_order(last.display_order)
                    .build(),
                None => self.data.clone(),
            };

            self.cursor = cursor;

            Ok(items)
        } else {
            Ok(vec![])
        }
    }

    pub async fn has_more(&self, tether: &Tether) -> Result<bool, StashError> {
        let all = self.data.visible_element_count(tether).await?;
        let cursor_count = self.cursor.visible_element_count(tether).await?;

        Ok(cursor_count < all)
    }
}

impl Deref for CachedConverstationScrollData {
    type Target = ConversationScrollData;

    fn deref(&self) -> &Self::Target {
        &self.cursor
    }
}

#[derive(Debug, Model, Eq, PartialEq, Clone, TypedBuilder)]
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
    #[builder(default, setter(strip_option))]
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
        let query = self.query(Some(1), true, None);
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
        let query = self.query(None, false, None);
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
        self.visible_elements_limit(None, None, tether).await
    }

    async fn visible_elements_limit(
        &self,
        limit: Option<usize>,
        offset: Option<u64>,
        tether: &Tether,
    ) -> Result<Vec<ContextualConversation>, StashError> {
        let query = self.query(limit, false, offset);
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

    fn query(&self, limit: Option<usize>, require_remote_id: bool, offset: Option<u64>) -> String {
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
            query += &format!(" LIMIT {limit} ");
        }

        if let Some(offset) = offset {
            query += &format!(" OFFSET {offset} ");
        }

        query
    }
}
