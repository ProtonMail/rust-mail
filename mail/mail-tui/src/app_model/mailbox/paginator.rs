use crate::app::Command;
use crate::app_model::watcher::WatchHandle;
use crate::messages::Messages;
use futures::future::BoxFuture;
use futures::FutureExt;
use proton_core_common::paginator::DataSource;
use proton_mail_common::models::PaginatorCompat;
use proton_mail_common::MailContextError;
use stash::orm::Model;
use stash::stash::{StashError, WatcherHandle};
use std::sync::Arc;

/// Paginator adapter.
pub struct Paginator<T: Model, R: DataSource<Item = T> + 'static> {
    paginator: Arc<PaginatorCompat<T, R>>,
    _watch_handle: WatchHandle,
}

impl<T: Model, R: DataSource<Item = T> + 'static> Paginator<T, R> {
    /// Create a new paginator instance.
    ///
    /// * `create_paginator` is a closure
    ///     that should create a paginator with the given `sender`.
    /// * `to_message` should convert the output of [`PaginatorCompat::reload`]
    ///     into a message.
    ///
    /// Creates a paginator and watcher.
    pub async fn new(
        creat_paginator: impl FnOnce() -> BoxFuture<
            'static,
            Result<PaginatorCompat<T, R>, MailContextError>,
        >,
        to_message: impl Fn(Result<Vec<T>, StashError>) -> Messages + Send + Sync + 'static,
    ) -> Result<(Self, Command<Messages>), MailContextError> {
        let to_message = Arc::new(to_message);
        let paginator = Arc::new(creat_paginator().await?);
        let WatcherHandle {
            handle, receiver, ..
        } = paginator.watch()?;
        let paginator_cloned = Arc::clone(&paginator);
        let (watcher, background_command) =
            WatchHandle::new_dampened(receiver, handle, move || {
                let paginator = Arc::clone(&paginator_cloned);
                let to_message = Arc::clone(&to_message);
                async move { Some(to_message(paginator.reload().await)) }.boxed()
            });
        Ok((
            Self {
                paginator,
                _watch_handle: watcher,
            },
            background_command,
        ))
    }

    /// Get the next pagination page.
    pub async fn next_page(&self) -> Result<Vec<T>, R::Error> {
        self.paginator.next_page().await
    }

    /// Get the next pagination page as series of background tasks which will
    /// display a message while the data is syncing.
    ///
    /// `to_command` should convert the output of [`next_page()`] to a command.
    pub fn next_page_command(
        &self,
        to_command: impl FnOnce(Vec<T>) -> Command<Messages> + Send + Sync + 'static,
    ) -> Command<Messages> {
        let paginator = Arc::clone(&self.paginator);
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Fetching next page...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    match paginator.next_page().await {
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
