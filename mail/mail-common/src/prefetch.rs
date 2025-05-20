use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use flume::Receiver;
use proton_action_queue::queue::Queue;
use proton_core_common::datatypes::{LocalLabelId, SystemLabel};
use proton_mail_ids::{LocalConversationId, LocalMessageId};
use stash::stash::{Tether, WatcherHandle};
use tracing::instrument;

use crate::{
    MailContextError, MailUserContext,
    actions::{conversations, messages},
    datatypes::{ReadFilter, ViewMode},
    mail_scroller::MailScroller,
    models::MailSettings,
};

macro_rules! prefetch_label_local_id {
    ($label:ident, $tether:expr) => {{
        let Ok(Some(local_id)) = SystemLabel::$label.local_id($tether).await else {
            tracing::error!("Failed to get local id for label {:?}", SystemLabel::$label);
            return vec![];
        };
        local_id
    }};
}

const PREFETCH_COUNT: usize = 10;
const PREVIOUS_PAGE_AWAIT_DURATION: Duration = Duration::from_secs(10);

pub type PrefetchNotify = OnceLock<flume::Sender<Vec<PrefetchJob>>>;

/// Prefetch component for downloading messages and conversations in the background.
pub struct Prefetch;

pub enum PrefetchJob {
    LocationCnvView(LocalLabelId),
    LocationMsgView(LocalLabelId),
    Conversation(LocalConversationId, LocalLabelId),
    Message(LocalMessageId),
}

impl PrefetchJob {
    pub async fn default_locations(tether: &Tether) -> Vec<Self> {
        let Ok(Some(mail_settings)) = MailSettings::get(tether).await else {
            tracing::error!("Failed to get mail settings");
            return vec![];
        };

        vec![
            match mail_settings.view_mode {
                ViewMode::Conversations => {
                    PrefetchJob::LocationCnvView(prefetch_label_local_id!(Inbox, &tether))
                }
                ViewMode::Messages => {
                    PrefetchJob::LocationMsgView(prefetch_label_local_id!(Inbox, &tether))
                }
            },
            PrefetchJob::LocationMsgView(prefetch_label_local_id!(Sent, &tether)),
            PrefetchJob::LocationMsgView(prefetch_label_local_id!(AllSent, &tether)),
            PrefetchJob::LocationMsgView(prefetch_label_local_id!(Drafts, &tether)),
            PrefetchJob::LocationMsgView(prefetch_label_local_id!(AllDrafts, &tether)),
        ]
    }
}

impl Prefetch {
    /// Start background task to prefetch messages and conversations
    ///
    /// It is looped and waiting for notification to start prefetching
    /// every call of this function will notify the task to start prefetching
    /// but the task will be executed only once per cycle.
    /// Meaning that if the task is already running, it will not be started again.
    ///
    /// If MailUserContext is dropped the background task will die.
    pub async fn initialize(ctx: Arc<MailUserContext>, reciever: Receiver<Vec<PrefetchJob>>) {
        let ctx_weak = Arc::downgrade(&ctx);

        ctx.spawn(async move {
            let ctx = ctx_weak;
            loop {
                let locations;

                if let Ok(message) = reciever.recv_async().await {
                    locations = message;
                } else {
                    break;
                }
                let Some(ctx) = ctx.upgrade() else {
                    break;
                };

                if locations.is_empty() {
                    tracing::debug!("Got an empty prefetch job list, skipping");
                } else {
                    let _ = Self::prefetch(ctx, &locations).await;
                }
            }
        });
    }

    /// Prefetch all defined locations one by one.
    #[instrument(skip(ctx, prefetch_locations))]
    async fn prefetch(
        ctx: Arc<MailUserContext>,
        prefetch_locations: &[PrefetchJob],
    ) -> Result<(), MailContextError> {
        let queue = ctx.action_queue();
        for location in prefetch_locations {
            match location {
                PrefetchJob::LocationCnvView(label_id) => {
                    tracing::debug!("Prefetching conversations for label {label_id}");
                    if let Err(error) = Self::prefetch_conversations(*label_id, &ctx, queue).await {
                        tracing::error!(
                            "Failed to prefetch conversations for label {label_id}, {error}",
                        );
                    }
                }
                PrefetchJob::LocationMsgView(label_id) => {
                    tracing::debug!("Prefetching messages for label {label_id}");
                    if let Err(error) = Self::prefetch_messages(*label_id, &ctx, queue).await {
                        tracing::error!(
                            "Failed to prefetch messages for label {label_id}, {error}",
                        );
                    }
                }
                PrefetchJob::Conversation(cnv_id, label_id) => {
                    tracing::debug!("Prefetch conversation {cnv_id}");
                    let action = conversations::Prefetch::new(*cnv_id, *label_id);
                    if let Err(error) = queue.queue_action(action).await {
                        tracing::error!("Failed to prefetch conversation {cnv_id}, {error}",);
                    }
                }
                PrefetchJob::Message(msg_id) => {
                    tracing::debug!("Prefetch message {msg_id}");
                    let action = messages::Prefetch::new(*msg_id);
                    if let Err(error) = queue.queue_action(action).await {
                        tracing::error!("Failed to prefetch message {msg_id}, {error}",);
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
    #[instrument(skip(ctx, queue))]
    async fn prefetch_conversations(
        local_label_id: LocalLabelId,
        ctx: &Arc<MailUserContext>,
        queue: &Queue,
    ) -> Result<(), MailContextError> {
        let Ok(mut scroller) = MailScroller::conversations(
            ctx.as_weak(),
            local_label_id,
            ReadFilter::All,
            PREFETCH_COUNT,
        )
        .await
        else {
            return Ok(());
        };
        let WatcherHandle {
            receiver,
            handle: _,
            ..
        } = scroller.watch().await?;
        // Wait for previous page just in case it arrives
        let _ = tokio::time::timeout(PREVIOUS_PAGE_AWAIT_DURATION, receiver.recv_async()).await;

        let items = scroller.fetch_more().await?;

        if items.is_empty() {
            return Ok(());
        }

        for item in items.into_iter().take(PREFETCH_COUNT) {
            let local_id = item.local_id;
            let action = conversations::Prefetch::new(local_id, local_label_id);
            if let Err(error) = queue.queue_action(action).await {
                tracing::error!("Failed to prefetch conversation {local_id}, {error}",);
            }
        }

        Ok(())
    }

    /// Prefetch messages for the given label.
    ///
    /// It fetches messages from the given label and prefetches the message body for each message.
    #[instrument(skip(ctx, queue))]
    async fn prefetch_messages(
        local_label_id: LocalLabelId,
        ctx: &Arc<MailUserContext>,
        queue: &Queue,
    ) -> Result<(), MailContextError> {
        let Ok(mut scroller) = MailScroller::messages(
            ctx.as_weak(),
            local_label_id,
            ReadFilter::All,
            PREFETCH_COUNT,
        )
        .await
        else {
            return Ok(());
        };
        let WatcherHandle {
            receiver,
            handle: _,
            ..
        } = scroller.watch().await?;
        // Wait for previous page just in case it arrives
        let _ = tokio::time::timeout(PREVIOUS_PAGE_AWAIT_DURATION, receiver.recv_async()).await;

        let items = scroller.fetch_more().await?;

        for item in items.into_iter().take(PREFETCH_COUNT) {
            let local_id = item.local_id.unwrap();
            let action = messages::Prefetch::new(local_id);
            if let Err(error) = queue.queue_action(action).await {
                tracing::error!("Failed to prefetch message {local_id}, {error}",);
            }
        }

        Ok(())
    }
}
