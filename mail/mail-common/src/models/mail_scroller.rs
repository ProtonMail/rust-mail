use super::{ConversationCounters, MessageCounters};
use crate::AppError;
use crate::datatypes::LocalMessageId;
use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
use crate::datatypes::{ContextualConversation, ReadFilter};
use crate::mail_scroller::MailScrollerItem;
use crate::models::{Conversation, ConversationLabel, Message, MessageLabel};
use anyhow::anyhow;
use indoc::formatdoc;
use proton_core_api::services::proton::ProtonIdMarker;
use proton_core_common::datatypes::{LocalLabelId, UnixTimestamp};
use proton_core_common::models::ModelExtension;
use proton_mail_api::services::proton::prelude::{ConversationId, MessageId};
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use std::fmt::Debug;
use std::future::Future;
use std::marker::PhantomData;
use std::ops::Deref;
use typed_builder::TypedBuilder;

pub trait ScrollData
where
    Self: Model + Into<ScrollCursor<Self>>,
{
    type Model: ModelExtension;
    type Item: MailScrollerItem;
    type RemoteId: ProtonIdMarker;

    fn find_with_key(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        order_dir: ScrollOrderDir,
        tether: &Tether,
    ) -> impl Future<Output = Result<Option<Self>, StashError>> + Send {
        async move {
            Self::find_first(
                "WHERE local_label_id=? AND unread=? AND order_dir=?",
                params![local_label_id, unread, order_dir],
                tether,
            )
            .await
        }
    }

    fn item_id(&self) -> Self::RemoteId;

    fn total(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        tether: &Tether,
    ) -> impl Future<Output = Result<u64, AppError>> + Send;

    #[allow(clippy::too_many_arguments)]
    fn query(
        unread: ReadFilter,
        limit: Option<usize>,
        offset: Option<u64>,
        require_remote_id: bool,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        time: UnixTimestamp,
        snooze_time: UnixTimestamp,
    ) -> String;

    fn convert(local_id: LocalLabelId, items: Vec<Self::Model>) -> Vec<Self::Item>;

    fn time(item: &Self::Item) -> UnixTimestamp;

    fn snooze_time(item: &Self::Item, order_field: ScrollOrderField) -> UnixTimestamp;

    fn display_order(item: &Self::Item) -> u64;

    fn into_scroll_data(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        item: Self::Item,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Option<Self>;

    fn watched_tables() -> Vec<String>;
}

#[derive(Debug, Model, Eq, PartialEq, Clone, TypedBuilder)]
#[TableName("mail_message_scroll_data")]
pub struct MessageScrollData {
    #[IdField(optional)]
    #[builder(default)]
    pub id: Option<u64>,

    #[DbField]
    pub local_label_id: LocalLabelId,

    #[DbField]
    pub unread: ReadFilter,

    #[DbField]
    pub remote_message_id: MessageId,

    #[DbField]
    pub message_time: UnixTimestamp,

    #[DbField]
    pub snooze_time: UnixTimestamp,

    #[DbField]
    pub display_order: u64,

    #[DbField]
    pub order_dir: ScrollOrderDir,

    #[DbField]
    pub order_field: ScrollOrderField,
}

impl MessageScrollData {
    pub async fn save(&mut self, tx: &Bond<'_>) -> Result<(), StashError> {
        if let Some(existing) =
            Self::find_with_key(self.local_label_id, self.unread, self.order_dir, tx).await?
        {
            self.id = existing.id;
        } else {
            // Trigger approach is problematic to implement for optional id field
            // in sql, so we use a workaround to get the next id manually.
            self.id = Some(Self::next_id(tx).await?);
        }

        <Self as Model>::save(self, tx).await?;

        Ok(())
    }

    pub fn context_time(&self, order_field: ScrollOrderField) -> UnixTimestamp {
        match order_field {
            ScrollOrderField::Time => self.message_time,
            ScrollOrderField::SnoozeTime => {
                if self.snooze_time.as_u64() > 0 {
                    self.snooze_time
                } else {
                    self.message_time
                }
            }
        }
    }
}

impl From<MessageScrollData> for ScrollCursor<MessageScrollData> {
    fn from(data: MessageScrollData) -> Self {
        Self {
            local_label_id: data.local_label_id,
            unread: data.unread,
            time: data.message_time,
            snooze_time: data.snooze_time,
            display_order: data.display_order,
            order_dir: data.order_dir,
            order_field: data.order_field,
            _phantom: PhantomData,
        }
    }
}

impl ScrollData for MessageScrollData {
    type Model = Message;
    type Item = Message;
    type RemoteId = MessageId;

    fn item_id(&self) -> MessageId {
        self.remote_message_id.clone()
    }

    async fn total(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        tether: &Tether,
    ) -> Result<u64, AppError> {
        let Some(counters) = MessageCounters::find_by_id(local_label_id, tether).await? else {
            return Err(AppError::LocalLabelHasNoCounters(local_label_id));
        };

        Ok(counters.total(unread))
    }

    fn query(
        unread: ReadFilter,
        limit: Option<usize>,
        offset: Option<u64>,
        require_remote_id: bool,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        time: UnixTimestamp,
        snooze_time: UnixTimestamp,
    ) -> String {
        //NOTE: we only check the display order for elements with matching time
        // or we will get incorrect query results.

        let (time_op, fallback_order_op, sort_op) = if order_dir == ScrollOrderDir::Desc {
            ('>', ">=", "DESC")
        } else {
            ('<', "<=", "ASC")
        };

        let time_column = match order_field {
            ScrollOrderField::Time => "messages.time".to_string(),
            ScrollOrderField::SnoozeTime => formatdoc!(
                "CASE WHEN messages.snooze_time > 0
                    THEN messages.snooze_time
                    ELSE messages.time END"
            ),
        };

        let cursor_constraint = match order_field {
            ScrollOrderField::Time => format!(
                "(
                {time_column} {time_op} {time}
                OR
                ({time_column} = {time} AND messages.display_order {fallback_order_op} ?2)
                )"
            ),
            ScrollOrderField::SnoozeTime => formatdoc!(
                "(
                {time_column} {time_op} {snooze_time}
                OR
                ({time_column} = {snooze_time}
                    AND messages.time {time_op} {time}
                )
                OR
                ({time_column} = {snooze_time}
                    AND messages.time = {time}
                    AND messages.display_order {fallback_order_op} ?2)
                )"
            ),
        };

        let mut query = formatdoc!(
            "
            JOIN message_labels
                ON messages.local_id = message_labels.local_message_id
            WHERE
                message_labels.local_label_id = ?1
            AND
                messages.deleted = 0
            AND {cursor_constraint}
            "
        );
        if require_remote_id {
            query += " AND messages.remote_id IS NOT NULL"
        }

        match unread {
            ReadFilter::All => {}
            ReadFilter::Unread => {
                query += " AND messages.unread = 1 ";
            }
            ReadFilter::Read => {
                query += " AND messages.unread = 0 ";
            }
        }

        let order_by = match order_field {
            ScrollOrderField::Time => format!(
                " ORDER BY
                {time_column} {sort_op},
                messages.display_order {sort_op}
            "
            ),
            ScrollOrderField::SnoozeTime => format!(
                " ORDER BY
                {time_column} {sort_op},
                messages.time {sort_op},
                messages.display_order {sort_op}
            "
            ),
        };

        query += &order_by;

        if let Some(limit) = limit {
            query += &format!(" LIMIT {limit} ");
        }

        if let Some(offset) = offset {
            query += &format!(" OFFSET {offset} ");
        }

        query
    }

    fn convert(_: LocalLabelId, items: Vec<Self::Model>) -> Vec<Self::Item> {
        items
    }

    fn time(item: &Self::Item) -> UnixTimestamp {
        item.time
    }

    fn snooze_time(item: &Self::Item, order_field: ScrollOrderField) -> UnixTimestamp {
        match order_field {
            ScrollOrderField::Time => item.time,
            ScrollOrderField::SnoozeTime => {
                if item.snooze_time.as_u64() > 0 {
                    item.snooze_time
                } else {
                    item.time
                }
            }
        }
    }

    fn display_order(item: &Self::Item) -> u64 {
        item.display_order
    }

    fn into_scroll_data(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        item: Self::Item,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Option<Self> {
        let time = Self::time(&item);
        let snooze_time = Self::snooze_time(&item, order_field);
        let display_order = Self::display_order(&item);

        if let Some(remote_id) = item.remote_id.clone() {
            return Some(
                MessageScrollData::builder()
                    .local_label_id(local_label_id)
                    .unread(unread)
                    .message_time(time)
                    .snooze_time(snooze_time)
                    .display_order(display_order)
                    .remote_message_id(remote_id)
                    .order_dir(order_dir)
                    .order_field(order_field)
                    .build(),
            );
        }

        None
    }

    fn watched_tables() -> Vec<String> {
        vec![
            Message::table_name().to_owned(),
            MessageLabel::table_name().to_owned(),
            MessageCounters::table_name().to_owned(),
        ]
    }
}

#[derive(Debug, Model, Eq, PartialEq, Clone, TypedBuilder)]
#[TableName("mail_conversation_scroll_data")]
pub struct ConversationScrollData {
    #[IdField(optional)]
    #[builder(default)]
    pub id: Option<u64>,

    #[DbField]
    pub local_label_id: LocalLabelId,

    #[DbField]
    pub unread: ReadFilter,

    #[DbField]
    pub remote_conversation_id: ConversationId,

    /// Note: for filtered conversation (`ReadFilter != ReadFilter::All`) we
    /// need to store the `Conversation.context_time` rather than
    /// `Conversation.Labels[active_label].context_time`
    #[DbField]
    pub conversation_time: UnixTimestamp,

    #[DbField]
    pub snooze_time: UnixTimestamp,

    #[DbField]
    pub display_order: u64,

    #[DbField]
    pub order_dir: ScrollOrderDir,

    #[DbField]
    pub order_field: ScrollOrderField,
}

impl ConversationScrollData {
    pub async fn save(&mut self, tx: &Bond<'_>) -> Result<(), StashError> {
        if let Some(existing) =
            Self::find_with_key(self.local_label_id, self.unread, self.order_dir, tx).await?
        {
            self.id = existing.id;
        } else {
            // Trigger approach is problematic to implement for optional id field
            // in sql, so we use a workaround to get the next id manually.
            self.id = Some(Self::next_id(tx).await?);
        }

        <Self as Model>::save(self, tx).await?;

        self.reload(tx).await?;

        Ok(())
    }

    pub fn context_time(&self, order_field: ScrollOrderField) -> UnixTimestamp {
        match order_field {
            ScrollOrderField::Time => self.conversation_time,
            ScrollOrderField::SnoozeTime => {
                if self.snooze_time.as_u64() > 0 {
                    self.snooze_time
                } else {
                    self.conversation_time
                }
            }
        }
    }
}

impl From<ConversationScrollData> for ScrollCursor<ConversationScrollData> {
    fn from(data: ConversationScrollData) -> Self {
        Self {
            local_label_id: data.local_label_id,
            unread: data.unread,
            time: data.conversation_time,
            snooze_time: data.snooze_time,
            display_order: data.display_order,
            order_dir: data.order_dir,
            order_field: data.order_field,
            _phantom: PhantomData,
        }
    }
}

impl ScrollData for ConversationScrollData {
    type Model = Conversation;
    type Item = ContextualConversation;
    type RemoteId = ConversationId;

    fn item_id(&self) -> ConversationId {
        self.remote_conversation_id.clone()
    }

    async fn total(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        tether: &Tether,
    ) -> Result<u64, AppError> {
        let Some(counters) = ConversationCounters::find_by_id(local_label_id, tether).await? else {
            return Err(AppError::LocalLabelHasNoCounters(local_label_id));
        };

        Ok(counters.total(unread))
    }

    fn query(
        unread: ReadFilter,
        limit: Option<usize>,
        offset: Option<u64>,
        require_remote_id: bool,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        time: UnixTimestamp,
        snooze_time: UnixTimestamp,
    ) -> String {
        let (time_op, fallback_order_op, sort_op) = if order_dir == ScrollOrderDir::Desc {
            ('>', ">=", "DESC")
        } else {
            ('<', "<=", "ASC")
        };

        let time_column = match order_field {
            ScrollOrderField::Time => "conversation_labels.context_time".to_string(),
            ScrollOrderField::SnoozeTime => {
                formatdoc!(
                    "CASE WHEN conversation_labels.context_snooze_time > 0
                        THEN conversation_labels.context_snooze_time
                        ELSE conversation_labels.context_time END"
                )
            }
        };

        let cursor_constraint = match order_field {
            ScrollOrderField::Time => format!(
                "(
                {time_column} {time_op} {time}
                OR
                ({time_column} = {time} AND conversations.display_order {fallback_order_op} ?2)
                )"
            ),
            ScrollOrderField::SnoozeTime => formatdoc!(
                "(
                {time_column} {time_op} {snooze_time}
                OR
                ({time_column} = {snooze_time}
                    AND conversation_labels.context_time {time_op} {time}
                )
                OR
                ({time_column} = {snooze_time}
                    AND conversation_labels.context_time = {time}
                    AND conversations.display_order {fallback_order_op} ?2)
                )"
            ),
        };

        let mut query = formatdoc!(
            "
            JOIN conversation_labels
                ON conversations.local_id = conversation_labels.local_conversation_id
            WHERE
                conversation_labels.local_label_id = ?1
            AND
                conversations.deleted = 0
            AND
                conversation_labels.deleted = 0
            AND {cursor_constraint}
            "
        );

        if require_remote_id {
            query += " AND conversations.remote_id IS NOT NULL"
        }

        match unread {
            ReadFilter::All => {}
            ReadFilter::Unread => {
                query += " AND conversation_labels.context_num_unread > 0 ";
            }
            ReadFilter::Read => {
                query += " AND conversation_labels.context_num_unread = 0 ";
            }
        }

        let order_by = match order_field {
            ScrollOrderField::Time => format!(
                " ORDER BY
                {time_column} {sort_op},
                conversations.display_order {sort_op}
            "
            ),
            ScrollOrderField::SnoozeTime => format!(
                " ORDER BY
                {time_column} {sort_op},
                conversation_labels.context_time {sort_op},
                conversations.display_order {sort_op}
            "
            ),
        };
        query += &order_by;

        if let Some(limit) = limit {
            query += &format!(" LIMIT {limit} ");
        }

        if let Some(offset) = offset {
            query += &format!(" OFFSET {offset} ");
        }

        query
    }

    fn convert(local_label_id: LocalLabelId, items: Vec<Self::Model>) -> Vec<Self::Item> {
        items
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, local_label_id))
            .collect()
    }

    fn time(item: &Self::Item) -> UnixTimestamp {
        item.time
    }

    fn snooze_time(item: &Self::Item, order_field: ScrollOrderField) -> UnixTimestamp {
        match order_field {
            ScrollOrderField::Time => item.time,
            ScrollOrderField::SnoozeTime => {
                if item.snooze_time.as_u64() > 0 {
                    item.snooze_time
                } else {
                    item.time
                }
            }
        }
    }

    fn display_order(item: &Self::Item) -> u64 {
        item.display_order
    }

    fn into_scroll_data(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        item: Self::Item,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Option<Self> {
        let time = Self::time(&item);
        let snooze_time = Self::snooze_time(&item, order_field);
        let display_order = Self::display_order(&item);

        if let Some(remote_id) = item.remote_id.clone() {
            return Some(
                ConversationScrollData::builder()
                    .local_label_id(local_label_id)
                    .unread(unread)
                    .conversation_time(time)
                    .snooze_time(snooze_time)
                    .display_order(display_order)
                    .remote_conversation_id(remote_id)
                    .order_dir(order_dir)
                    .order_field(order_field)
                    .build(),
            );
        }

        None
    }

    fn watched_tables() -> Vec<String> {
        vec![
            Conversation::table_name().to_owned(),
            ConversationLabel::table_name().to_owned(),
            ConversationCounters::table_name().to_owned(),
        ]
    }
}

#[derive(Debug, Eq, PartialEq, Clone, TypedBuilder)]
pub struct ScrollCursor<T: ScrollData> {
    pub local_label_id: LocalLabelId,
    pub unread: ReadFilter,
    pub time: UnixTimestamp,
    pub snooze_time: UnixTimestamp,
    pub display_order: u64,
    pub order_dir: ScrollOrderDir,
    pub order_field: ScrollOrderField,

    #[builder(default)]
    pub _phantom: PhantomData<T>,
}

impl<T: ScrollData> ScrollCursor<T> {
    pub fn absolute_beginning(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Self {
        if order_dir == ScrollOrderDir::Desc {
            Self::highest(local_label_id, unread, order_dir, order_field)
        } else {
            Self::lowest(local_label_id, unread, order_dir, order_field)
        }
    }

    pub fn absolute_end(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Self {
        if order_dir == ScrollOrderDir::Asc {
            Self::highest(local_label_id, unread, order_dir, order_field)
        } else {
            Self::lowest(local_label_id, unread, order_dir, order_field)
        }
    }

    fn highest(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Self {
        ScrollCursor {
            local_label_id,
            unread,
            time: (i64::MAX as u64).into(),
            snooze_time: (i64::MAX as u64).into(),
            display_order: i64::MAX as u64,
            order_dir,
            order_field,
            _phantom: PhantomData,
        }
    }

    fn lowest(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Self {
        ScrollCursor {
            local_label_id,
            unread,
            time: 0.into(),
            snooze_time: 0.into(),
            display_order: 0,
            order_dir,
            order_field,
            _phantom: PhantomData,
        }
    }

    /// Same as [`visible_elements`] but returns only the number of items that match.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    ///
    pub async fn seen_count(&self, tether: &Tether) -> Result<u64, StashError> {
        ScrollQuery::new(self.clone()).count(tether).await
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
    ///
    pub async fn visible_elements(&self, tether: &Tether) -> Result<Vec<T::Item>, StashError> {
        self.visible_elements_limit(None, None, false, tether).await
    }

    /// Internal function to get the visible elements with limit and offset.
    ///
    async fn visible_elements_limit(
        &self,
        limit: Option<usize>,
        offset: Option<u64>,
        require_remote_id: bool,
        tether: &Tether,
    ) -> Result<Vec<T::Item>, StashError> {
        ScrollQuery::new(self.clone())
            .with_limit(limit)
            .with_offset(offset)
            .with_remote_id(require_remote_id)
            .find(tether)
            .await
    }
}

/// In memory cache for buffered read of the ScrollData.
///
/// This is useful for offline mode and for performance reasons as it buffers loading
/// of data from the database. This comes crucial whene switching between views
/// and in order to not load all available items everytime we do utilize this cache.
///
#[derive(Debug, Clone)]
pub struct CachedScrollData<T: ScrollData> {
    page_size: usize,
    end: ScrollCursor<T>,
    cursor: ScrollCursor<T>,
}

impl<T: ScrollData> CachedScrollData<T> {
    /// Create a new cache for generic ScrollData.
    ///
    /// This will load the data from the database and create a cursor for the
    /// generic ScrollData in the place where first page should end.
    ///
    /// # Returns
    ///
    /// A cursor when the data is found, otherwise `None` as the view was never displayed before.
    ///
    /// # Arguments
    ///
    /// `local_label_id` - The local label id of the label in which the scroll is performed.
    /// `unread` - The read filter used in the scroll.
    /// `page_size` - The size of the page to load.
    /// `tether` - The tether to use for the database access.
    ///
    /// # Errors
    ///
    /// Specific to database access.
    ///
    pub async fn new(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        let order_dir = ScrollOrderDir::for_local_label(local_label_id, tether).await?;
        let order_field = ScrollOrderField::for_local_label(local_label_id, tether).await?;

        let data = T::find_with_key(local_label_id, unread, order_dir, tether).await?;

        Ok(match data {
            Some(data) => {
                let end = data.into();
                let cursor = ScrollCursor::absolute_beginning(
                    local_label_id,
                    unread,
                    order_dir,
                    order_field,
                );

                Some(Self {
                    page_size,
                    end,
                    cursor,
                })
            }
            None => None,
        })
    }

    /// Create a new cache for generic ScrollData.
    ///
    /// This will load all available data from the database and create a cursor for the
    /// generic ScrollData in the place where first page should end.
    ///
    /// # Returns
    ///
    /// A cursor when the data is found, otherwise `None` as the view was never displayed before.
    ///
    /// # Arguments
    ///
    /// `local_label_id` - The local label id of the label in which the scroll is performed.
    /// `unread` - The read filter used in the scroll.
    /// `page_size` - The size of the page to load.
    /// `tether` - The tether to use for the database access.
    ///
    /// # Errors
    ///
    /// Specific to database access.
    ///
    pub fn all(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Self {
        let end = ScrollCursor::absolute_end(local_label_id, unread, order_dir, order_field);

        let cursor =
            ScrollCursor::absolute_beginning(local_label_id, unread, order_dir, order_field);

        Self {
            page_size,
            end,
            cursor,
        }
    }

    /// Transform the cursor to read absolutly all items from the database.
    pub fn set_absolute_end(mut self) -> Self {
        self.end = ScrollCursor::absolute_end(
            self.cursor.local_label_id,
            self.cursor.unread,
            self.end.order_dir,
            self.end.order_field,
        );
        self
    }

    /// Fetch more items from the database.
    ///
    /// This will load the next page of items from the database and update the cursor.
    /// If there are no more items to load, an empty vector is returned.
    /// In case the cursor is at the one before the last page.
    /// It will load two pages instead of one if the last page is not completly filled.
    ///
    pub async fn fetch_more(&mut self, tether: &Tether) -> Result<Vec<T::Item>, StashError> {
        let all = self.end.seen_count(tether).await?;
        let cursor_count = self.cursor.seen_count(tether).await?;

        if cursor_count < all {
            let offset = Some(cursor_count);
            let remaining = all - cursor_count;
            let double_page = self.page_size as u64 * 2;
            let limit = if remaining < double_page {
                // Progress two pages at a time if there are less than two pages left.
                usize::try_from(all - cursor_count)
                    .ok()
                    .or(Some(self.page_size))
            } else {
                Some(self.page_size)
            };

            self.fetch_more_impl(limit, offset, tether).await
        } else {
            Ok(vec![])
        }
    }

    /// Fetch more items from the database, optimized to use with `while` loop.
    ///
    /// The method returns `None` for empty vector and the last page logic from
    /// [`fetch_more`] is omitted to return at most page_size worth of items,
    /// which makes it very easy to write:
    ///
    /// ```ignore
    /// while let Some(page) = scroller.while_fetch_more(tether).await? { ... }
    /// ```
    ///
    pub async fn while_fetch_more(
        &mut self,
        tether: &Tether,
    ) -> Result<Option<Vec<T::Item>>, StashError> {
        let all = self.end.seen_count(tether).await?;
        let cursor_count = self.cursor.seen_count(tether).await?;

        if cursor_count < all {
            let offset = Some(cursor_count);
            let limit = Some(self.page_size);
            let items = self.fetch_more_impl(limit, offset, tether).await?;

            if items.is_empty() {
                Ok(None)
            } else {
                Ok(Some(items))
            }
        } else {
            Ok(None)
        }
    }

    async fn fetch_more_impl(
        &mut self,
        limit: Option<usize>,
        offset: Option<u64>,
        tether: &Tether,
    ) -> Result<Vec<T::Item>, StashError> {
        let items = self
            .end
            .visible_elements_limit(limit, offset, false, tether)
            .await?;

        let cursor = match items.last() {
            Some(last) => ScrollCursor::builder()
                .local_label_id(self.local_label_id)
                .unread(self.unread)
                .time(T::time(last))
                .snooze_time(T::snooze_time(last, self.end.order_field))
                .display_order(T::display_order(last))
                .order_dir(self.end.order_dir)
                .order_field(self.end.order_field)
                .build(),
            None => self.end.clone(),
        };

        self.cursor = cursor;

        Ok(items)
    }

    /// Available elements count to fetch with this cursor
    ///
    pub async fn synced_count(&self, tether: &Tether) -> Result<u64, StashError> {
        self.end.seen_count(tether).await
    }

    /// Check if there are more items to fetch for in memory cursor.
    ///
    pub async fn has_more(&self, tether: &Tether) -> Result<bool, StashError> {
        let all = self.end.seen_count(tether).await?;
        let cursor_count = self.cursor.seen_count(tether).await?;

        Ok(cursor_count < all)
    }

    /// Check if there is a next page to fetch for in memory cursor.
    ///
    pub async fn has_next_page(&self, tether: &Tether) -> Result<bool, StashError> {
        let all = self.end.seen_count(tether).await?;
        let cursor_count = self.cursor.seen_count(tether).await?;

        if all > cursor_count {
            Ok(all - cursor_count >= self.page_size as u64)
        } else {
            Ok(false)
        }
    }

    /// Update the cache with the latest data from the database.
    ///
    /// It is very handy for ever-changing environment where the data in the database
    /// is downloaded in another thread. We may want to move the "end_cursor" - `data`
    /// further to the end of the downloaded list of elements.
    ///
    pub async fn update(&mut self, tether: &Tether) -> Result<(), StashError> {
        self.end = self.load_end_cursor(tether).await?.into();

        Ok(())
    }

    pub async fn scroll_data_begin(&self, tether: &Tether) -> Result<Option<T>, StashError> {
        let first = self
            .end
            .visible_elements_limit(Some(1), None, true, tether)
            .await?
            .pop();

        match first {
            Some(first) => Ok(T::into_scroll_data(
                self.local_label_id,
                self.unread,
                first,
                self.order_dir,
                self.order_field,
            )),
            None => Ok(None),
        }
    }

    pub async fn scroll_data_end(&self, tether: &Tether) -> Result<Option<T>, StashError> {
        let cursor_count = self.synced_count(tether).await?.saturating_sub(1);
        let last = self
            .end
            .visible_elements_limit(Some(1), Some(cursor_count), true, tether)
            .await?
            .pop();
        match last {
            Some(last) => Ok(T::into_scroll_data(
                self.local_label_id,
                self.unread,
                last,
                self.order_dir,
                self.order_field,
            )),
            None => Ok(None),
        }
    }

    /// Get the underlying "data" to which the end cursor points to.
    ///
    pub async fn load_end_cursor(&self, tether: &Tether) -> Result<T, StashError> {
        // Due to nature of primary key of the underlying table
        // It does not really matter if we take end or cursor as
        // they should be the same however `end` var is just shorter.
        let end = &self.end;

        T::find_with_key(end.local_label_id, end.unread, end.order_dir, tether)
            .await
            .and_then(|op| {
                op.ok_or_else(|| {
                    StashError::Critical(anyhow!(
                        "Non-generic ScrollData not found for label_id: {}, \
                     unread: {:?}. This is serious issue.",
                        end.local_label_id,
                        end.unread
                    ))
                })
            })
    }
}

impl<T: ScrollData> Deref for CachedScrollData<T> {
    type Target = ScrollCursor<T>;

    fn deref(&self) -> &Self::Target {
        &self.cursor
    }
}

#[derive(Debug, Model, Eq, PartialEq, Clone, TypedBuilder)]
#[TableName("mail_search_scroll_data")]
pub struct SearchScrollData {
    #[IdField]
    pub local_message_id: LocalMessageId,

    #[DbField]
    pub display_order: u64,
}

impl SearchScrollData {
    pub async fn last(tether: &Tether) -> Result<Option<Self>, StashError> {
        SearchScrollData::find_first("ORDER BY display_order DESC", vec![], tether).await
    }

    pub async fn last_remote_message_id_and_time(
        tether: &Tether,
    ) -> Result<Option<(MessageId, UnixTimestamp)>, StashError> {
        let Some(last) = Self::last(tether).await? else {
            return Ok(None);
        };

        let message = last.remote_message(tether).await?;
        let retval = message
            .and_then(|message| message.remote_id.map(|remote_id| (remote_id, message.time)));

        debug_assert!(retval.is_some());

        Ok(retval)
    }

    pub async fn remote_message(&self, tether: &Tether) -> Result<Option<Message>, StashError> {
        let message = Message::find_by_id(self.local_message_id, tether).await?;
        Ok(message)
    }

    pub async fn has_more(&self, tether: &Tether) -> Result<bool, StashError> {
        let last = Self::last(tether).await?;

        Ok(match last {
            Some(last) => last.display_order > self.display_order,
            None => false,
        })
    }

    pub async fn fetch_more(
        &mut self,
        page_size: usize,
        tether: &Tether,
    ) -> Result<Vec<Message>, StashError> {
        let last = Self::last(tether).await?;

        if let Some(last) = last
            && last.display_order > self.display_order
        {
            let offset = self.display_order.saturating_add(1);

            let query = Self::query(Some(page_size), Some(offset));
            let items = Message::find(query, params![last.display_order], tether).await?;
            *self = last;

            return Ok(items);
        }

        Ok(vec![])
    }
    /// Same as [`visible_elements`] but returns only the number of items that match.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    ///
    pub async fn visible_element_count(&self, tether: &Tether) -> Result<u64, StashError> {
        let query = Self::query(None, None);
        Message::count(query, params![self.display_order], tether).await
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
    ///
    pub async fn visible_elements(&self, tether: &Tether) -> Result<Vec<Message>, StashError> {
        self.visible_elements_limit(None, None, tether).await
    }

    /// Internal function to get the visible elements with limit and offset.
    ///
    async fn visible_elements_limit(
        &self,
        limit: Option<usize>,
        offset: Option<u64>,
        tether: &Tether,
    ) -> Result<Vec<Message>, StashError> {
        let query = Self::query(limit, offset);

        Message::find(query, params![self.display_order], tether).await
    }

    fn query(limit: Option<usize>, offset: Option<u64>) -> String {
        //NOTE: we only check the display order for elements with matching time
        // or we will get incorrect query results.
        let mut query = formatdoc!(
            "
            JOIN mail_search_scroll_data
                ON messages.local_id = mail_search_scroll_data.local_message_id
            AND
                messages.deleted = 0
            AND
                mail_search_scroll_data.display_order <= ?
            "
        );

        query += " ORDER BY
            mail_search_scroll_data.display_order ASC
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

pub struct ScrollQuery<T: ScrollData> {
    cursor: ScrollCursor<T>,
    limit: Option<usize>,
    offset: Option<u64>,
    require_remote_id: bool,
}

impl<T: ScrollData> ScrollQuery<T> {
    pub fn new(cursor: ScrollCursor<T>) -> Self {
        Self {
            cursor,
            limit: None,
            offset: None,
            require_remote_id: false,
        }
    }

    pub fn with_limit(mut self, limit: impl Into<Option<usize>>) -> Self {
        self.limit = limit.into();
        self
    }

    pub fn with_offset(mut self, offset: impl Into<Option<u64>>) -> Self {
        self.offset = offset.into();
        self
    }

    pub fn with_remote_id(mut self, remote_id: bool) -> Self {
        self.require_remote_id = remote_id;
        self
    }

    pub async fn find(&self, tether: &Tether) -> Result<Vec<T::Item>, StashError> {
        let query = T::query(
            self.cursor.unread,
            self.limit,
            self.offset,
            self.require_remote_id,
            self.cursor.order_dir,
            self.cursor.order_field,
            self.cursor.time,
            self.cursor.snooze_time,
        );
        let items = T::Model::find(
            query,
            params![self.cursor.local_label_id, self.cursor.display_order],
            tether,
        )
        .await?;

        Ok(T::convert(self.cursor.local_label_id, items))
    }

    pub async fn count(&self, tether: &Tether) -> Result<u64, StashError> {
        let query = T::query(
            self.cursor.unread,
            None,
            None,
            self.require_remote_id,
            self.cursor.order_dir,
            self.cursor.order_field,
            self.cursor.time,
            self.cursor.snooze_time,
        );

        T::Model::count(
            query,
            params![self.cursor.local_label_id, self.cursor.display_order],
            tether,
        )
        .await
    }
}
