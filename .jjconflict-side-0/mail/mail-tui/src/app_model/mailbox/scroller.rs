use crate::app::Command;
use crate::app_model::watcher::TuiWatchHandle;
use crate::messages::Messages;
use futures::FutureExt;
use proton_mail_common::{
    MailScroller as RealMailScroller, MailScrollerHandle, MailScrollerItem, ScrollerUpdate,
};
use std::{ops::Deref, sync::Arc};

pub struct MailScroller<T>
where
    T: MailScrollerItem,
{
    scoller: Arc<RealMailScroller<T>>,
    pub supports_include_filter: bool,
    _watch_handle: TuiWatchHandle,
}

impl<T> MailScroller<T>
where
    T: MailScrollerItem,
{
    pub async fn new<U>(
        scoller: RealMailScroller<T>,
        handle: MailScrollerHandle<U>,
        to_message: impl Fn(ScrollerUpdate<U>) -> Messages + Send + Sync + 'static,
    ) -> (Self, Command<Messages>)
    where
        U: Send + 'static,
    {
        let scoller = Arc::new(scoller);
        let supports_include_filter = scoller.supports_include_filter().await.unwrap_or(false);
        let to_message = Arc::new(to_message);

        let (watcher, background_command) =
            TuiWatchHandle::new(handle.updates, handle.handle, move |update| {
                let to_message = Arc::clone(&to_message);

                async move { Some(to_message(update)) }.boxed()
            });

        (
            Self {
                scoller,
                supports_include_filter,
                _watch_handle: watcher,
            },
            background_command,
        )
    }

    pub fn clone_inner(&self) -> Arc<RealMailScroller<T>> {
        Arc::clone(&self.scoller)
    }

    pub async fn total(&self) -> u64 {
        self.scoller.total().await.unwrap()
    }

    pub fn fetch_more(&self) {
        let _ = self.scoller.fetch_more(None);
    }
}

impl<T> Deref for MailScroller<T>
where
    T: MailScrollerItem,
{
    type Target = RealMailScroller<T>;

    fn deref(&self) -> &Self::Target {
        &self.scoller
    }
}
