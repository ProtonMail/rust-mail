//! Paginator for managing large result sets with live updates.
//!
//! For more information, see the [`RealPaginator`] struct.
//!

use crate::core::datatypes::Id;
use crate::mail::datatypes::{Conversation, Message};
use crate::mail::MailboxError;
use crate::{async_runtime, uniffi_async, WatchHandle};
use itertools::Itertools;
use proton_mail_common::datatypes::ContextualConversation;
use proton_mail_common::models::{Conversation as RealConversation, Message as RealMessage};
use stash::paginator::Paginator as RealPaginator;
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
    pub(crate) real_paginator: RealPaginator<RealConversation>,

    /// The handle to stop watching the data.
    pub(crate) handle: Arc<WatchHandle>,

    /// The local ID of the label.
    pub(crate) label_id: Id,
}

#[uniffi::export]
impl ConversationPaginator {
    /// Retrieves the results of the current page.
    ///
    /// # Errors
    ///
    /// Returns an error if the current page could not be fetched from the
    /// database.
    ///
    pub async fn current_page(self: Arc<Self>) -> Result<Vec<Conversation>, MailboxError> {
        uniffi_async(async move {
            Ok(self
                .real_paginator
                .current_page()
                .await?
                .into_iter()
                .filter_map(|c| ContextualConversation::new(c, self.label_id.into()))
                .map_into()
                .collect())
        })
        .await
    }

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

    /// Moves to the previous page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page before the previous page could not be
    /// fetched from the database.
    ///
    pub async fn previous_page(self: Arc<Self>) -> Result<Vec<Conversation>, MailboxError> {
        uniffi_async(async move {
            Ok(self
                .real_paginator
                .previous_page()
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
    pub fn result_count(&self) -> u32 {
        async_runtime().block_on(async { self.real_paginator.result_count().await })
    }

    /// Retrieves the current page number.
    #[must_use]
    pub fn current_page_number(&self) -> u32 {
        async_runtime().block_on(async { self.real_paginator.current_page_number().await })
    }

    /// Retrieves the total number of pages.
    #[must_use]
    pub fn page_count(&self) -> u32 {
        async_runtime().block_on(async { self.real_paginator.page_count().await })
    }

    /// Checks if there is a next page available.
    #[must_use]
    pub fn has_next_page(&self) -> bool {
        async_runtime().block_on(async { self.real_paginator.has_next_page().await })
    }

    /// Checks if there is a previous page available.
    #[must_use]
    pub fn has_previous_page(&self) -> bool {
        async_runtime().block_on(async { self.real_paginator.has_previous_page().await })
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
    pub(crate) real_paginator: RealPaginator<RealMessage>,

    /// The handle to stop watching the data.
    pub(crate) handle: Arc<WatchHandle>,
}

#[uniffi::export]
impl MessagePaginator {
    /// Retrieves the results of the current page.
    ///
    /// # Errors
    ///
    /// Returns an error if the current page could not be fetched from the
    /// database.
    ///
    pub async fn current_page(self: Arc<Self>) -> Result<Vec<Message>, MailboxError> {
        uniffi_async(async move {
            Ok(self
                .real_paginator
                .current_page()
                .await?
                .iter()
                .map(|m| m.clone().into())
                .collect())
        })
        .await
    }

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

    /// Moves to the previous page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page before the previous page could not be
    /// fetched from the database.
    ///
    pub async fn previous_page(self: Arc<Self>) -> Result<Vec<Message>, MailboxError> {
        uniffi_async(async move {
            Ok(self
                .real_paginator
                .previous_page()
                .await?
                .iter()
                .map(|m| m.clone().into())
                .collect())
        })
        .await
    }

    /// Retrieves the total number of records in the result set.
    #[must_use]
    pub fn result_count(&self) -> u32 {
        async_runtime().block_on(async { self.real_paginator.result_count().await })
    }

    /// Retrieves the current page number.
    #[must_use]
    pub fn current_page_number(&self) -> u32 {
        async_runtime().block_on(async { self.real_paginator.current_page_number().await })
    }

    /// Retrieves the total number of pages.
    #[must_use]
    pub fn page_count(&self) -> u32 {
        async_runtime().block_on(async { self.real_paginator.page_count().await })
    }

    /// Checks if there is a next page available.
    #[must_use]
    pub fn has_next_page(&self) -> bool {
        async_runtime().block_on(async { self.real_paginator.has_next_page().await })
    }

    /// Checks if there is a previous page available.
    #[must_use]
    pub fn has_previous_page(&self) -> bool {
        async_runtime().block_on(async { self.real_paginator.has_previous_page().await })
    }
}
