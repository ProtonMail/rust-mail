use std::sync::{Arc, OnceLock};

use crate::datatypes::{LocalConversationId, LocalMessageId};
use flume::Receiver;
use proton_core_common::datatypes::LocalLabelId;
use tracing::instrument;

use crate::{
    MailContextError, MailUserContext,
    actions::{conversations, messages},
};

pub type PrefetchNotify = OnceLock<flume::Sender<Vec<PrefetchJob>>>;

/// Prefetch component for downloading messages and conversations in the background.
pub struct Prefetch;

pub enum PrefetchJob {
    Conversation(LocalConversationId, LocalLabelId),
    Message(LocalMessageId),
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
}

/// App origin only
#[derive(Default)]
pub struct PrefetchService {
    pub notify: PrefetchNotify,
}

impl PrefetchService {
    pub fn new() -> Self {
        Self::default()
    }
}
