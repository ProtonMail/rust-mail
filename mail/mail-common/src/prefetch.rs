use std::sync::{Arc, Weak};

use flume::Receiver;
use proton_core_common::{
    datatypes::{LocalLabelId, SystemLabel},
    models::Label,
};
use stash::{orm::Model, stash::Tether};
use tokio::task::yield_now;
use tracing::instrument;

use crate::{
    datatypes::{ReadFilter, ViewMode},
    mail_scroller::MailScroller,
    models::{Conversation, MailSettings, Message},
    MailContextError, MailUserContext,
};

/// Prefetch component for downloading messages and conversations in the background.
pub struct Prefetch {
    prefetch_count: usize,
    prefetch_locations: Vec<Location>,
}

enum Location {
    Conversations(LocalLabelId),
    Messages(LocalLabelId),
}

macro_rules! label_local_id {
    ($label:ident, $tether:expr) => {{
        let Ok(Some(local_id)) = SystemLabel::$label.local_id($tether).await else {
            tracing::error!("Failed to get local id for label {:?}", SystemLabel::$label);
            return;
        };
        local_id
    }};
}

impl Prefetch {
    /// Start background task to prefetch messages and conversations
    ///
    /// It is looped and waiting for notification to start prefetching
    /// every call of this function will notify the task to start prefetching
    /// but the task will be executed only once per cycle.
    /// Meaning that if the task is already running, it will not be started again.
    ///
    /// If MailUserContext is dropped
    pub async fn initialize(ctx: Arc<MailUserContext>, reciever: Receiver<()>) {
        let prefetch_count = 10;
        let tether = ctx.user_stash().connection();

        let Ok(Some(mail_settings)) = MailSettings::get(&tether).await else {
            tracing::error!("Failed to get mail settings");
            return;
        };

        let locations = vec![
            match mail_settings.view_mode {
                ViewMode::Conversations => Location::Conversations(label_local_id!(Inbox, &tether)),
                ViewMode::Messages => Location::Messages(label_local_id!(Inbox, &tether)),
            },
            Location::Messages(label_local_id!(Sent, &tether)),
            Location::Messages(label_local_id!(AllSent, &tether)),
            Location::Messages(label_local_id!(Drafts, &tether)),
            Location::Messages(label_local_id!(AllDrafts, &tether)),
        ];

        let this = Self {
            prefetch_count,
            prefetch_locations: locations,
        };

        tokio::spawn(async move {
            let ctx = Arc::downgrade(&ctx);
            loop {
                if reciever.recv_async().await.is_err() {
                    break;
                }
                let _ = this.prefetch(&ctx).await;
                drop(reciever.drain());
            }
        });
    }

    /// Prefetch all defined locations one by one.
    #[instrument(skip(self, ctx))]
    async fn prefetch(&self, ctx: &Weak<MailUserContext>) -> Result<(), MailContextError> {
        let ctx = ctx.upgrade().unwrap();
        let mut tether = ctx.user_stash().connection();

        for location in &self.prefetch_locations {
            yield_now().await;
            match location {
                Location::Conversations(label_id) => {
                    tracing::debug!("Prefetching conversations for label {:?}", label_id);
                    if let Err(error) = self
                        .prefetch_conversations(*label_id, &mut tether, &ctx)
                        .await
                    {
                        tracing::error!(
                            "Failed to prefetch conversations for label {:?}, {error}",
                            label_id
                        );
                    }
                }
                Location::Messages(label_id) => {
                    tracing::debug!("Prefetching messages for label {:?}", label_id);
                    if let Err(error) = self.prefetch_messages(*label_id, &ctx).await {
                        tracing::error!(
                            "Failed to prefetch messages for label {:?}, {error}",
                            label_id
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Prefetch conversations for the given label.
    ///
    /// It fetches conversations from the given label and prefetches all message metadata
    /// tied to it and finally downloads the message to open body for each conversation.
    #[instrument(skip(self, tether, ctx))]
    async fn prefetch_conversations(
        &self,
        local_label_id: LocalLabelId,
        tether: &mut Tether,
        ctx: &Arc<MailUserContext>,
    ) -> Result<(), MailContextError> {
        let Ok(mut scroller) =
            MailScroller::conversations(ctx.clone(), local_label_id, ReadFilter::All, 50).await
        else {
            return Ok(());
        };
        yield_now().await;

        let items = scroller.fetch_more().await?;

        yield_now().await;
        for item in items.into_iter().take(self.prefetch_count) {
            let api = ctx.api();
            let _ = Conversation::sync_conversation_messages(item.local_id, tether, api).await;
            yield_now().await;
            let messages = Message::in_conversation(item.local_id, tether).await?;
            yield_now().await;
            let Some(label) = Label::load(local_label_id, tether).await? else {
                continue;
            };
            let Ok(message_id_to_open) =
                Conversation::message_id_to_open(item.local_id, &label, &messages)
            else {
                continue;
            };
            yield_now().await;
            tracing::debug!(
                "Prefetching message {:?} body for conversation {:?}",
                message_id_to_open,
                item.local_id
            );
            let _ = Message::message_body(ctx.clone(), message_id_to_open).await;
            yield_now().await;
        }

        Ok(())
    }

    /// Prefetch messages for the given label.
    ///
    /// It fetches messages from the given label and prefetches the message body for each message.
    #[instrument(skip(self, ctx))]
    async fn prefetch_messages(
        &self,
        local_label_id: LocalLabelId,
        ctx: &Arc<MailUserContext>,
    ) -> Result<(), MailContextError> {
        let Ok(mut scroller) =
            MailScroller::messages(ctx.clone(), local_label_id, ReadFilter::All, 50).await
        else {
            return Ok(());
        };
        yield_now().await;
        let items = scroller.fetch_more().await?;
        yield_now().await;
        for item in items.into_iter().take(self.prefetch_count) {
            tracing::debug!("Prefetching message {:?} body", item.local_id);
            let _ = Message::message_body(ctx.clone(), item.local_id.unwrap()).await;
            yield_now().await;
        }

        Ok(())
    }
}
