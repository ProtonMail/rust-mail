use crate::app::Command;
use crate::app_model::watcher::TuiWatchHandle;
use crate::messages::Messages;
use futures::FutureExt;
use proton_mail_common::{
    mail_scroller::{MailScroller, MailScrollerHandle, ScrollerUpdate},
    traits::ScrollerEq,
};
use std::sync::Arc;

/// Paginator adapter.
pub struct Paginator {
    paginator: Arc<MailScroller>,
    _watch_handle: TuiWatchHandle,
}

impl Paginator {
    /// Create a new paginator instance.
    ///
    /// * `create_paginator` is a closure
    ///   that should create a paginator with the given `sender`.
    /// * `to_message` should convert the output of [`PaginatorCompat::reload`]
    ///   into a message.
    ///
    /// Creates a paginator and watcher.
    pub fn new<T: Send + Sync + Clone + ScrollerEq + 'static>(
        paginator: MailScroller,
        handle: MailScrollerHandle<T>,
        to_message: impl Fn(ScrollerUpdate<T>) -> Messages + Send + Sync + 'static,
    ) -> (Self, Command<Messages>) {
        let paginator = Arc::new(paginator);
        let to_message = Arc::new(to_message);
        let (watcher, background_command) =
            TuiWatchHandle::new(handle.updates, handle.handle, move |update| {
                let to_message = Arc::clone(&to_message);
                async move { Some(to_message(update)) }.boxed()
            });
        (
            Self {
                paginator,
                _watch_handle: watcher,
            },
            background_command,
        )
    }

    pub fn clone_inner(&self) -> Arc<MailScroller> {
        Arc::clone(&self.paginator)
    }

    pub async fn total(&self) -> u64 {
        self.paginator.total().await.unwrap()
    }

    /// Get the next pagination page as series of background tasks which will
    /// display a message while the data is syncing.
    ///
    /// `to_command` should convert the output of [`next_page()`] to a command.
    pub fn next_page_command(&self) -> Command<Messages> {
        let _ = self.paginator.fetch_more();
        Command::None
    }
}
