//! Paginator for managing large result sets with live updates.
//!
//! For more information, see the [`RealPaginator`] struct.
//!

use crate::mail::datatypes::Message;
use crate::mail::MailboxError;
use crate::WatchHandle;
use proton_mail_common::models::Message as RealMessage;
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
pub struct MessagePaginator {
    /// The "real" paginator that does the heavy lifting.
    pub(crate) real_paginator: RealPaginator<RealMessage>,

    /// The handle to stop watching the data.
    pub(crate) handle: Arc<WatchHandle>,
}

impl MessagePaginator {
    /// Retrieves the results of the current page.
    ///
    /// # Errors
    ///
    /// Returns an error if the current page could not be fetched from the
    /// database.
    ///
    pub async fn current_page(&self) -> Result<Vec<Message>, MailboxError> {
        Ok(self
            .real_paginator
            .current_page()
            .await?
            .iter()
            .map(|m| m.clone().into())
            .collect())
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
    pub async fn next_page(&self) -> Result<Vec<Message>, MailboxError> {
        Ok(self
            .real_paginator
            .next_page()
            .await?
            .iter()
            .map(|m| m.clone().into())
            .collect())
    }

    /// Moves to the previous page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page before the previous page could not be
    /// fetched from the database.
    ///
    pub async fn previous_page(&self) -> Result<Vec<Message>, MailboxError> {
        Ok(self
            .real_paginator
            .previous_page()
            .await?
            .iter()
            .map(|m| m.clone().into())
            .collect())
    }

    /// Retrieves the total number of records in the result set.
    pub async fn result_count(&self) -> u32 {
        self.real_paginator.result_count().await
    }

    /// Retrieves the current page number.
    pub async fn current_page_number(&self) -> u32 {
        self.real_paginator.current_page_number().await
    }

    /// Retrieves the total number of pages.
    pub async fn page_count(&self) -> u32 {
        self.real_paginator.page_count().await
    }

    /// Checks if there is a next page available.
    pub async fn has_next_page(&self) -> bool {
        self.real_paginator.has_next_page().await
    }

    /// Checks if there is a previous page available.
    pub async fn has_previous_page(&self) -> bool {
        self.real_paginator.has_previous_page().await
    }
}
