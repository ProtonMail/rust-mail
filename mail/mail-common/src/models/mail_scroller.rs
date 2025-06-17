use super::{ConversationCounters, MessageCounters};
use crate::AppError;
use crate::datatypes::labels::LabelScrollOrder;
use crate::datatypes::{ContextualConversation, ReadFilter};
use crate::models::{Conversation, ConversationLabel, Message, MessageLabel};
use anyhow::anyhow;
use indoc::formatdoc;
use proton_core_common::datatypes::{LocalLabelId, UnixTimestamp};
use proton_core_common::models::ModelExtension;
use proton_mail_api::services::proton::prelude::{ConversationId, MessageId};
use proton_mail_ids::LocalMessageId;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use std::future::Future;
use std::ops::Deref;
use typed_builder::TypedBuilder;

/// Trait defining the scroll data.
///
/// Extends Model and requires conversion to ScrollCursor.
pub trait ScrollData: Model + Into<ScrollCursor<Self>> {
    /// Model of the Data
    type Model: ModelExtension;
    /// Item type returned by the Data
    type Item: Send;

    /// Find the first record with matching:
    /// * label_id,
    /// * read_filter
    ///
    fn find_with_key(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        scroll_order: LabelScrollOrder,
        tether: &Tether,
    ) -> impl Future<Output = Result<Option<Self>, StashError>> + Send {
        async move {
            Self::find_first(
                "WHERE local_label_id=? AND unread=? AND scroll_order=?",
                params![local_label_id, unread, scroll_order],
                tether,
            )
            .await
        }
    }

    /// Total number of items to load from the database.
    /// Implementator should use underlying counters structure to deterimn
    /// How many items in total are there to paginate over.
    fn total(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        tether: &Tether,
    ) -> impl Future<Output = Result<u64, AppError>> + Send;

    /// Query to get the data of associated type Model from the database.
    ///
    /// # Arguments
    /// * filter - determin the read/unread/all status of items to paginate over
    /// * limit - limit the number of items to load
    /// * require_remote_id - if the remote_id is required for the item
    ///   this parameter ensures that remote_id is defined in database
    ///   so the item can be used to request more pages
    /// * offset - offset of the items to load, it is used for loading cached partial pages
    ///
    fn query(
        filter: ReadFilter,
        limit: Option<usize>,
        require_remote_id: bool,
        offset: Option<u64>,
        order: LabelScrollOrder,
    ) -> String;

    /// Conversion between associated types of Model and Item.
    fn convert(local_id: LocalLabelId, items: Vec<Self::Model>) -> Vec<Self::Item>;

    /// Get the time of the item.
    fn time(item: &Self::Item) -> UnixTimestamp;

    /// Get the display order of the item.
    fn display_order(item: &Self::Item) -> u64;

    /// Transform model into ScrollData
    fn into_scroll_data(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        item: Self::Item,
        scroll_order: LabelScrollOrder,
    ) -> Option<Self>;

    /// List of tables that are watched by the scroll data.
    fn watched_tables() -> Vec<String>;
}

#[derive(Debug, Model, Eq, PartialEq, Clone, TypedBuilder)]
#[TableName("mail_message_scroll_data")]
pub struct MessageScrollData {
    #[IdField]
    pub local_label_id: LocalLabelId,

    #[DbField]
    pub unread: ReadFilter,

    #[DbField]
    pub remote_message_id: MessageId,

    #[DbField]
    pub message_time: UnixTimestamp,

    #[DbField]
    pub display_order: u64,

    #[DbField]
    pub scroll_order: LabelScrollOrder,

    #[RowIdField]
    #[builder(default, setter(strip_option))]
    pub row_id: Option<u64>,
}

impl MessageScrollData {
    pub async fn save(&mut self, tx: &Bond<'_>) -> Result<(), StashError> {
        // NOTE: save should always update existing records.
        // But as long as we have no support for multiple records as a key
        // we have to first delete the record and then save it.
        if let Some(existing) =
            Self::find_with_key(self.local_label_id, self.unread, self.scroll_order, tx).await?
        {
            self.row_id = existing.row_id;
            if self != &existing {
                existing.delete(tx).await?;
                self.row_id = None;
                <Self as Model>::save(self, tx).await?;
            }
        } else {
            <Self as Model>::save(self, tx).await?;
        }

        Ok(())
    }
}

impl From<MessageScrollData> for ScrollCursor<MessageScrollData> {
    fn from(data: MessageScrollData) -> Self {
        Self {
            local_label_id: data.local_label_id,
            unread: data.unread,
            time: data.message_time,
            display_order: data.display_order,
            scroll_order: data.scroll_order,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl ScrollData for MessageScrollData {
    type Model = Message;
    type Item = Message;

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
        filter: ReadFilter,
        limit: Option<usize>,
        require_remote_id: bool,
        offset: Option<u64>,
        scroll_order: LabelScrollOrder,
    ) -> String {
        //NOTE: we only check the display order for elements with matching time
        // or we will get incorrect query results.

        let (time_compare_op, display_order_compare_op) =
            if scroll_order == LabelScrollOrder::Descending {
                ('>', ">=")
            } else {
                ('<', "<=")
            };

        let mut query = formatdoc!(
            "
            JOIN message_labels
                ON messages.local_id = message_labels.local_message_id
            WHERE
                message_labels.local_label_id = ?1
            AND
                messages.deleted = 0
            AND (
                    messages.time {time_compare_op} ?2
                OR
                    (messages.time = ?2 AND messages.display_order {display_order_compare_op} ?3)
                )
            "
        );
        if require_remote_id {
            query += " AND messages.remote_id IS NOT NULL"
        }

        match filter {
            ReadFilter::All => {}
            ReadFilter::Unread => {
                query += " AND messages.unread = 1 ";
            }
            ReadFilter::Read => {
                query += " AND messages.unread = 0 ";
            }
        }

        if scroll_order == LabelScrollOrder::Ascending {
            query += " ORDER BY
            messages.time ASC,
            messages.display_order ASC
        ";
        } else {
            query += " ORDER BY
            messages.time DESC,
            messages.display_order DESC
        ";
        }

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

    fn display_order(item: &Self::Item) -> u64 {
        item.display_order
    }

    fn into_scroll_data(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        item: Self::Item,
        scroll_order: LabelScrollOrder,
    ) -> Option<Self> {
        let time = Self::time(&item);
        let display_order = Self::display_order(&item);
        if let Some(remote_id) = item.remote_id.clone() {
            return Some(
                MessageScrollData::builder()
                    .local_label_id(local_label_id)
                    .unread(unread)
                    .message_time(time)
                    .display_order(display_order)
                    .remote_message_id(remote_id)
                    .scroll_order(scroll_order)
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
    #[IdField]
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
    pub display_order: u64,

    #[DbField]
    pub scroll_order: LabelScrollOrder,

    #[RowIdField]
    #[builder(default, setter(strip_option))]
    pub row_id: Option<u64>,
}

impl ConversationScrollData {
    pub async fn save(&mut self, tx: &Bond<'_>) -> Result<(), StashError> {
        // NOTE: save should always update existing records.
        // But as long as we have no support for multiple records as a key
        // we have to first delete the record and then save it.
        if let Some(existing) =
            Self::find_with_key(self.local_label_id, self.unread, self.scroll_order, tx).await?
        {
            self.row_id = existing.row_id;
            if self != &existing {
                existing.delete(tx).await?;
                self.row_id = None;
                <Self as Model>::save(self, tx).await?;
            }
        } else {
            <Self as Model>::save(self, tx).await?;
        }

        Ok(())
    }
}

impl From<ConversationScrollData> for ScrollCursor<ConversationScrollData> {
    fn from(data: ConversationScrollData) -> Self {
        Self {
            local_label_id: data.local_label_id,
            unread: data.unread,
            time: data.conversation_time,
            display_order: data.display_order,
            scroll_order: LabelScrollOrder::default(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl ScrollData for ConversationScrollData {
    type Model = Conversation;
    type Item = ContextualConversation;

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
        filter: ReadFilter,
        limit: Option<usize>,
        require_remote_id: bool,
        offset: Option<u64>,
        scroll_order: LabelScrollOrder,
    ) -> String {
        let (time_compare_op, display_order_compare_op) =
            if scroll_order == LabelScrollOrder::Descending {
                ('>', ">=")
            } else {
                ('<', "<=")
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
            AND (
                    conversation_labels.context_time {time_compare_op} ?2
                OR
                    (conversation_labels.context_time = ?2 AND conversations.display_order {display_order_compare_op} ?3)
                )
            "
        );
        if require_remote_id {
            query += " AND conversations.remote_id IS NOT NULL"
        }

        match filter {
            ReadFilter::All => {}
            ReadFilter::Unread => {
                query += " AND conversation_labels.context_num_unread > 0 ";
            }
            ReadFilter::Read => {
                query += " AND conversation_labels.context_num_unread = 0 ";
            }
        }

        if scroll_order == LabelScrollOrder::Ascending {
            query += " ORDER BY
            conversation_labels.context_time ASC,
            conversations.display_order ASC
        ";
        } else {
            query += " ORDER BY
            conversation_labels.context_time DESC,
            conversations.display_order DESC
        ";
        }

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

    fn display_order(item: &Self::Item) -> u64 {
        item.display_order
    }

    fn into_scroll_data(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        item: Self::Item,
        scroll_order: LabelScrollOrder,
    ) -> Option<Self> {
        let time = Self::time(&item);
        let display_order = Self::display_order(&item);
        if let Some(remote_id) = item.remote_id.clone() {
            return Some(
                ConversationScrollData::builder()
                    .local_label_id(local_label_id)
                    .unread(unread)
                    .conversation_time(time)
                    .display_order(display_order)
                    .remote_conversation_id(remote_id)
                    .scroll_order(scroll_order)
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
    /// Label id used in the sync.
    pub local_label_id: LocalLabelId,

    /// Read filter used in the sync.
    pub unread: ReadFilter,

    /// Last synced item time.
    pub time: UnixTimestamp,

    /// Last synced display order.
    pub display_order: u64,

    pub scroll_order: LabelScrollOrder,

    #[builder(default)]
    pub _phantom: std::marker::PhantomData<T>,
}

impl<T: ScrollData> ScrollCursor<T> {
    /// Create a new ScrollCursor set to the very begining of the data.
    pub fn absolute_beginning(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        scroll_order: LabelScrollOrder,
    ) -> Self {
        if scroll_order == LabelScrollOrder::Descending {
            Self::highest(local_label_id, unread, scroll_order)
        } else {
            Self::lowest(local_label_id, unread, scroll_order)
        }
    }

    /// Create a new ScrollCursor set to the very end of the data.
    pub fn absolute_end(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        scroll_order: LabelScrollOrder,
    ) -> Self {
        if scroll_order == LabelScrollOrder::Ascending {
            Self::highest(local_label_id, unread, scroll_order)
        } else {
            Self::lowest(local_label_id, unread, scroll_order)
        }
    }

    fn highest(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        scroll_order: LabelScrollOrder,
    ) -> Self {
        ScrollCursor {
            local_label_id,
            unread,
            time: (i64::MAX as u64).into(),
            display_order: i64::MAX as u64,
            scroll_order,
            _phantom: std::marker::PhantomData,
        }
    }
    fn lowest(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        scroll_order: LabelScrollOrder,
    ) -> Self {
        ScrollCursor {
            local_label_id,
            unread,
            time: 0.into(),
            display_order: 0,
            scroll_order,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Same as [`visible_elements`] but returns only the number of items that match.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    ///
    pub async fn seen_count(&self, tether: &Tether) -> Result<u64, StashError> {
        let query = T::query(self.unread, None, false, None, self.scroll_order);
        T::Model::count(
            query,
            params![self.local_label_id, self.time, self.display_order],
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
        let query = T::query(
            self.unread,
            limit,
            require_remote_id,
            offset,
            self.scroll_order,
        );
        Ok(T::convert(
            self.local_label_id,
            T::Model::find(
                query,
                params![self.local_label_id, self.time, self.display_order],
                tether,
            )
            .await?,
        ))
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
        let scroll_order = LabelScrollOrder::for_local_label_id(local_label_id, tether).await?;
        let data = T::find_with_key(local_label_id, unread, scroll_order, tether).await?;

        Ok(match data {
            Some(data) => {
                let end = data.into();
                let cursor = ScrollCursor::absolute_beginning(local_label_id, unread, scroll_order);

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
        scroll_order: LabelScrollOrder,
    ) -> Self {
        let end = ScrollCursor::absolute_end(local_label_id, unread, scroll_order);
        let cursor = ScrollCursor::absolute_beginning(local_label_id, unread, scroll_order);

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
            self.end.scroll_order,
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
                .display_order(T::display_order(last))
                .scroll_order(self.end.scroll_order)
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

    /// Check if there are more than a page worth of items to fetch for in memory cursor.
    ///
    pub async fn has_more_than_a_page(&self, tether: &Tether) -> Result<bool, StashError> {
        let all = self.end.seen_count(tether).await?;
        let cursor_count = self.cursor.seen_count(tether).await?;

        if all > cursor_count {
            Ok(all - cursor_count > self.page_size as u64)
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
        self.end = self.scroll_data_end(tether).await?.into();

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
                self.scroll_order,
            )),
            None => Ok(None),
        }
    }

    /// Get the underlying "data" to which the end cursor points to.
    ///
    pub async fn scroll_data_end(&self, tether: &Tether) -> Result<T, StashError> {
        // Due to nature of primary key of the underlying table
        // It does not really matter if we take end or cursor as
        // they should be the same however `end` var is just shorter.
        let end = &self.end;

        T::find_with_key(end.local_label_id, end.unread, end.scroll_order,tether)
            .await
            .and_then(|op| {
                op.ok_or_else(|| {
                    StashError::Critical(anyhow!(
                        "Non-generic ScrollData not found for label_id: {}, unread: {:?}. This is serious issue.",
                        end.local_label_id, end.unread
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

    #[RowIdField]
    #[builder(default, setter(strip_option))]
    pub row_id: Option<u64>,
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

        if let Some(last) = last {
            if last.display_order > self.display_order {
                let offset = self.display_order.saturating_add(1);

                let query = Self::query(Some(page_size), Some(offset));
                let items = Message::find(query, params![last.display_order], tether).await?;
                *self = last;

                return Ok(items);
            }
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
