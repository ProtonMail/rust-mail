//! Paginator for managing large result sets with live updates.
//!
//! For more information, see the [`RealPaginator`] struct.
//!

use crate::core::datatypes::Id;
use crate::mail::datatypes::{Conversation, Message};
use crate::mail::MailboxError;
use crate::{async_runtime, uniffi_async, WatchHandle};
use itertools::Itertools;
use proton_core_common::paginator::Paginator as RealPaginator;
use proton_mail_common::datatypes::ContextualConversation;
use proton_mail_common::models::{
    Conversation as RealConversation, ConversationDataSource, Message as RealMessage,
    MessageDataSource,
};
use std::sync::Arc;

/// Represents a paginated view of a result set.
///
/// The [`Paginator`] manages the result set, providing pagination capabilities
/// and handling live updates. It can be used for both paginated and
/// non-paginated result sets, offering a consistent interface for data access.
///
/// It manages a sliding window of results, pre-fetching adjacent pages for
/// quick access while maintaining a consistent view of the data even as it
/// changes. It handles live updates, cursor management, and provides an
/// intuitive navigation experience through the result set.
///
#[derive(uniffi::Object)]
pub struct ConversationPaginator {
    /// The "real" paginator that does the heavy lifting.
    pub(crate) real_paginator: RealPaginator<RealConversation, ConversationDataSource>,

    /// The handle to stop watching the data.
    pub(crate) handle: Arc<WatchHandle>,

    /// The local ID of the label.
    pub(crate) label_id: Id,
}

#[uniffi::export]
impl ConversationPaginator {
    /// Retrieves the handle to stop watching the data.
    #[must_use]
    pub fn handle(&self) -> Arc<WatchHandle> {
        Arc::clone(&self.handle)
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page after the next page could not be fetched
    /// from the database.
    ///
    pub async fn next_page(self: Arc<Self>) -> Result<Vec<Conversation>, MailboxError> {
        uniffi_async(async move {
            Ok(self
                .real_paginator
                .next_page()
                .await?
                .into_iter()
                .filter_map(|c| ContextualConversation::new(c, self.label_id.into()))
                .map_into()
                .collect())
        })
        .await
    }

    /// Reloads all data up to the cursor.
    ///
    /// Grabs **ALL** the rows that have been seen so far, without any kind of
    /// limit or pagination, from the start right up to the current cursor
    /// position.
    ///
    /// This does not attempt to prefetch anything, and does not update any
    /// pagination state data.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be fetched from the database.
    ///
    pub async fn reload(self: Arc<Self>) -> Result<Vec<Conversation>, MailboxError> {
        uniffi_async(async move {
            Ok(self
                .real_paginator
                .reload()
                .await?
                .into_iter()
                .filter_map(|c| ContextualConversation::new(c, self.label_id.into()))
                .map_into()
                .collect())
        })
        .await
    }

    /// Retrieves the total number of records in the result set.
    #[must_use]
    pub fn result_count(&self) -> u64 {
        async_runtime().block_on(async { self.real_paginator.result_count().await }) as u64
    }
}

/// Represents a paginated view of a result set.
///
/// The [`Paginator`] manages the result set, providing pagination capabilities
/// and handling live updates. It can be used for both paginated and
/// non-paginated result sets, offering a consistent interface for data access.
///
/// It manages a sliding window of results, pre-fetching adjacent pages for
/// quick access while maintaining a consistent view of the data even as it
/// changes. It handles live updates, cursor management, and provides an
/// intuitive navigation experience through the result set.
///
#[derive(uniffi::Object)]
pub struct MessagePaginator {
    /// The "real" paginator that does the heavy lifting.
    pub(crate) real_paginator: RealPaginator<RealMessage, MessageDataSource>,

    /// The handle to stop watching the data.
    pub(crate) handle: Arc<WatchHandle>,
}

#[uniffi::export]
impl MessagePaginator {
    /// Retrieves the handle to stop watching the data.
    #[must_use]
    pub fn handle(&self) -> Arc<WatchHandle> {
        Arc::clone(&self.handle)
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page after the next page could not be fetched
    /// from the database.
    ///
    pub async fn next_page(self: Arc<Self>) -> Result<Vec<Message>, MailboxError> {
        uniffi_async(async move {
            Ok(self
                .real_paginator
                .next_page()
                .await?
                .iter()
                .map(|m| m.clone().into())
                .collect())
        })
        .await
    }

    /// Reloads all data up to the cursor.
    ///
    /// Grabs **ALL** the rows that have been seen so far, without any kind of
    /// limit or pagination, from the start right up to the current cursor
    /// position.
    ///
    /// This does not attempt to prefetch anything, and does not update any
    /// pagination state data.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be fetched from the database.
    ///
    pub async fn reload(self: Arc<Self>) -> Result<Vec<Message>, MailboxError> {
        uniffi_async(async move {
            Ok(self
                .real_paginator
                .reload()
                .await?
                .iter()
                .map(|m| m.clone().into())
                .collect())
        })
        .await
    }

    /// Retrieves the total number of records in the result set.
    #[must_use]
    pub fn result_count(&self) -> u64 {
        async_runtime().block_on(async { self.real_paginator.result_count().await }) as u64
    }
}
