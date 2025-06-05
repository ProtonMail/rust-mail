use crate::app::Command;
use crate::app_model::watcher::TuiWatchHandle;
use crate::messages::Messages;
use futures::FutureExt;
use proton_mail_common::MailContextError;
use proton_mail_common::mail_scroller::{MailScroller, MailScrollerSource};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Paginator adapter.
pub struct Paginator<T: MailScrollerSource + 'static> {
    paginator: Arc<Mutex<MailScroller<T>>>,
    _watch_handle: TuiWatchHandle,
}

impl<T: MailScrollerSource> Paginator<T> {
    /// Create a new paginator instance.
    ///
    /// * `create_paginator` is a closure
    ///   that should create a paginator with the given `sender`.
    /// * `to_message` should convert the output of [`PaginatorCompat::reload`]
    ///   into a message.
    ///
    /// Creates a paginator and watcher.
    pub async fn new(
        mut paginator: MailScroller<T>,
        to_message: impl Fn(Result<Vec<T::Item>, MailContextError>) -> Messages + Send + Sync + 'static,
    ) -> Result<(Self, Command<Messages>), MailContextError> {
        let handle = paginator.watch().await?;
        let paginator = Arc::new(Mutex::new(paginator));

        let paginator_cloned = Arc::clone(&paginator);
        let to_message = Arc::new(to_message);
        let (watcher, background_command) =
            TuiWatchHandle::from_watcher_handle(handle, move || {
                let paginator = Arc::clone(&paginator_cloned);
                let to_message = Arc::clone(&to_message);
                async move { Some(to_message(paginator.lock().await.all_items().await)) }.boxed()
            });
        Ok((
            Self {
                paginator,
                _watch_handle: watcher,
            },
            background_command,
        ))
    }

    pub fn clone_paginator(&self) -> Arc<Mutex<MailScroller<T>>> {
        Arc::clone(&self.paginator)
    }

    pub async fn fetch_more(&self) -> Result<Vec<T::Item>, MailContextError> {
        self.paginator.lock().await.fetch_more().await
    }

    pub async fn total(&self) -> u64 {
        self.paginator.lock().await.total()
    }

    /// Get the next pagination page as series of background tasks which will
    /// display a message while the data is syncing.
    ///
    /// `to_command` should convert the output of [`next_page()`] to a command.
    pub fn next_page_command(
        &self,
        to_command: impl FnOnce(Vec<T::Item>) -> Command<Messages> + Send + Sync + 'static,
    ) -> Command<Messages> {
        let paginator = Arc::clone(&self.paginator);
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Fetching next page...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    match paginator.lock().await.fetch_more().await {
                        Ok(v) => to_command(v),
                        Err(e) => Command::message(Messages::DisplayError(
                            Some("Paginator Next Page Failed".to_owned()),
                            anyhow::anyhow!("{e}"),
                        )),
                    },
                    Command::message(Messages::DismissBackgroundProgress),
                ])
            }),
        ])
    }
}
