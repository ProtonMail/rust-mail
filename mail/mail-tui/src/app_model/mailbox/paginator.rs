use crate::app::Command;
use crate::app_model::watcher::TuiWatchHandle;
use crate::messages::Messages;
use futures::FutureExt;
use proton_mail_common::mail_scroller::{MailScroller, MailScrollerHandle, ScrollerUpdate};
use std::{ops::Deref, sync::Arc};

pub struct Paginator {
    paginator: Arc<MailScroller>,
    _watch_handle: TuiWatchHandle,
}

impl Paginator {
    pub fn new<T>(
        paginator: MailScroller,
        handle: MailScrollerHandle<T>,
        to_message: impl Fn(ScrollerUpdate<T>) -> Messages + Send + Sync + 'static,
    ) -> (Self, Command<Messages>)
    where
        T: Send + 'static,
    {
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

    pub fn next_page_command(&self) -> Command<Messages> {
        let _ = self.paginator.fetch_more();

        Command::None
    }
}

impl Deref for Paginator {
    type Target = MailScroller;

    fn deref(&self) -> &Self::Target {
        &self.paginator
    }
}
