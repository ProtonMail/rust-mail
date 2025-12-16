use crate::actions::refresh::ActionRefresh;
use crate::datatypes::ConversationLabelsCount;
use crate::datatypes::MessageLabelsCount;
use crate::datatypes::{LocalConversationId, LocalMessageId};
use crate::models::IncomingDefault;
use crate::prefetch::PrefetchJob;
use crate::user_context::events::conversations::handle_conversation_events;
use crate::user_context::events::labels::handle_counters_label_events;
use crate::user_context::events::messages::handle_message_events;
use crate::{AppError, MailContextError, MailUserContext};
use anyhow::Context;
use async_trait::async_trait;
use proton_action_queue::action::ActionGroup;
use proton_action_queue::queue::{ActionError as QueueActionError, QueuedActionOutput};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::service::ApiServiceError;
use proton_core_common::datatypes::{Refresh, SystemLabel};
use proton_core_common::models::LabelError;
use proton_event_loop::RefreshFlag;
use stash::orm::Model;
use std::sync::{Arc, Weak};
use tracing::{debug, error, info, warn};
// Import common macros from core
use crate::datatypes::dependencies::MessageOrConversationDependencyFetcher;
use crate::events::v6;
use crate::user_context::events::event_model::MailEvent;
use crate::user_context::events::event_source::MailEventSourceV5;
use proton_event_loop::v6::{
    EventSource, EventSubscriber, EventSubscriberError, EventSubscriberResult,
};
use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};
use proton_mail_api::services::proton::prelude::MailEventV5;
use stash::stash::{StashError, Tether};

pub struct MailEventV5Subscriber(Weak<MailUserContext>);

impl MailEventV5Subscriber {
    pub fn new(ctx: Weak<MailUserContext>) -> Self {
        Self(ctx)
    }
}

/// This is data to be used outside of the transaction context
#[derive(Default)]
pub struct PostEventSyncData {
    pub msg_for_prefetch: Vec<LocalMessageId>,
    pub cnv_for_prefetch: Vec<LocalConversationId>,
    queue_incoming_default: bool,
}

impl PostEventSyncData {
    pub fn update_incoming_default(&mut self) {
        self.queue_incoming_default = true;
    }
    pub async fn apply(
        self,
        ctx: &Arc<MailUserContext>,
        tether: &Tether,
    ) -> Result<(), MailContextError> {
        let label_id = SystemLabel::AllMail.local_id(tether).await?.unwrap();

        let conversation_jobs = self
            .cnv_for_prefetch
            .into_iter()
            .map(|id| PrefetchJob::Conversation(id, label_id))
            .collect();

        let message_jobs = self
            .msg_for_prefetch
            .into_iter()
            .map(PrefetchJob::Message)
            .collect();

        let _ = ctx
            .queue_prefetch_jobs(conversation_jobs)
            .await
            .inspect_err(|e| {
                error!("Failed to queue cnv jobs for prefetch: {e}");
            });

        let _ = ctx
            .queue_prefetch_jobs(message_jobs)
            .await
            .inspect_err(|e| {
                error!("Failed to queue msg jobs for prefetch: {e}");
            });

        if self.queue_incoming_default {
            IncomingDefault::action_resync(ctx.action_queue()).await;
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MailEventSubscriberError {
    #[error(transparent)]
    Api(#[from] ApiServiceError),
    #[error(transparent)]
    Stash(#[from] StashError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<LabelError> for MailEventSubscriberError {
    fn from(e: LabelError) -> Self {
        match e {
            LabelError::API(e) => Self::Api(e),
            LabelError::Stash(e) => Self::Stash(e),
            e => Self::Other(e.into()),
        }
    }
}

impl From<MailContextError> for MailEventSubscriberError {
    fn from(e: MailContextError) -> Self {
        match e {
            MailContextError::Label(e) => e.into(),
            MailContextError::App(e) => e.into(),
            MailContextError::Stash(e) => Self::Stash(e),
            MailContextError::Api(e) => Self::Api(e),
            _ => Self::Other(e.into()),
        }
    }
}

impl From<AppError> for MailEventSubscriberError {
    fn from(e: AppError) -> Self {
        match e {
            AppError::API(e) => Self::Api(e),
            AppError::Stash(e) => Self::Stash(e),
            AppError::Label(e) => e.into(),
            e => Self::Other(e.into()),
        }
    }
}

impl EventSubscriberError for MailEventSubscriberError {
    fn is_network_failure(&self) -> bool {
        match self {
            MailEventSubscriberError::Api(e) => e.is_network_failure(),
            MailEventSubscriberError::Stash(_) | MailEventSubscriberError::Other(_) => false,
        }
    }
    fn is_retryable(&self) -> bool {
        match self {
            MailEventSubscriberError::Api(e) => e.is_network_failure() || e.is_server_failure(),
            MailEventSubscriberError::Stash(_) | MailEventSubscriberError::Other(_) => false,
        }
    }
}

#[async_trait]
impl EventSubscriber<MailEventSourceV5> for MailEventV5Subscriber {
    fn name(&self) -> &'static str {
        "proton-mail-event-subscriber"
    }

    async fn on_event(
        &self,
        event: &MailEventV5,
        _: &mut <MailEventSourceV5 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        async {
            let Some(ctx) = self.0.upgrade() else {
                warn!("Mail user context is no longer alive");
                return Ok(());
            };

            // TODO: to be replaced with fetching of elements from API.
            let mut event: MailEvent = event.clone().into();

            let mut tether = ctx.user_context.stash().connection().await?;
            let mut data = PostEventSyncData::default();

            // Check for missing dependencies. Sometimes when lot of messages/conversations get moved
            // to newly created label the items can have the new label before the label create event.

            calculate_missing_dependencies(&event, &tether)
                .await
                .context("Failed to calculate dependencies")?
                .fetch_and_store(ctx.session(), &mut tether)
                .await
                .inspect_err(|e| error!("Failed to fetch or store dependencies: {e}"))?;

            tether
                .tx::<_, _, MailEventSubscriberError>(async |tx| {
                    let mut rebase_change_set = RebaseChangeSet::default();

                    if let Some(labels) = &event.labels {
                        debug!("Handling label events");
                        handle_counters_label_events(tx, labels)
                            .await
                            .context("Error handling label events")?;
                    }

                    if let Some(ref mut conversations) = event.conversations {
                        debug!("Handling conversation events");
                        handle_conversation_events(
                            tx,
                            conversations,
                            &mut rebase_change_set,
                            &mut data,
                        )
                        .await
                        .context("Error handling conversation events")?;
                    }

                    if let Some(ref mut messages) = event.messages {
                        debug!("Handling message events");
                        handle_message_events(tx, messages, &mut rebase_change_set, &mut data)
                            .await
                            .context("Error handling message events")?;
                    }

                    if let Some(conversation_counts) = &event.conversation_counts {
                        debug!("Handling conversation counts");
                        ConversationLabelsCount::create_or_update_conversation_counts(
                            conversation_counts.clone(),
                            tx,
                        )
                        .await?;
                    }

                    if let Some(message_counts) = &event.message_counts {
                        debug!("Handling message counts");
                        MessageLabelsCount::create_or_update_message_counts(
                            message_counts.clone(),
                            tx,
                        )
                        .await?;
                    }

                    if let Some(mail_settings) = event.mail_settings.as_mut() {
                        debug!("Handling mail settings");
                        mail_settings.save(tx).await?;
                    }

                    // It so happens that the API only returns the IDs of what changed, not the
                    // actual data, so we better reload all.
                    if event.incoming_defaults.is_some() {
                        data.update_incoming_default()
                    }

                    ctx.rebaseable_queue()
                        .await
                        .rebase_in(ActionGroup::default(), &rebase_change_set, tx)
                        .await
                        .context("Failed to rebase")?;
                    Ok(())
                })
                .await
                .inspect_err(|e| {
                    ctx.issue_reporter_service().report(
                        IssueLevel::Error,
                        "Failed to apply mail events".into(),
                        issue_report_keys_from_error(e),
                    )
                })
                .context("Failed to apply changes")?;

            data.apply(&ctx, &tether).await?;

            Ok::<_, MailEventSubscriberError>(())
        }
        .await
        .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }

    async fn on_refresh(
        &self,
        refresh_flag: RefreshFlag,
        _: &mut <MailEventSourceV5 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("Mail user context is no longer alive");
            return Ok(());
        };
        ctx.on_refresh_impl(refresh_flag.as_u8().into()).await
    }
}

impl MailUserContext {
    pub async fn refresh_action(
        &self,
        refresh: impl Into<Refresh>,
    ) -> Result<QueuedActionOutput<ActionRefresh>, QueueActionError<ActionRefresh>> {
        self.action_queue()
            .queue_action(ActionRefresh::new(refresh.into()))
            .await
    }

    pub async fn on_refresh_impl(&self, refresh: Refresh) -> EventSubscriberResult<()> {
        info!("Handling refresh event: {refresh:?}");

        match refresh {
            Refresh::None => {
                warn!("Nothing to refresh, this may idicate bug in SDK event loop implementation");
            }
            Refresh::Mail | Refresh::All => v6::refresh_mail(self)
                .await
                .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })?,
            Refresh::Contacts => {
                // Contacts refresh is handled by the core event subscriber
            }
            Refresh::Unknown(other) => {
                warn!("Unknown refresh event type: {other}");
            }
        }

        Ok(())
    }
}

async fn calculate_missing_dependencies(
    event: &MailEvent,
    tether: &Tether,
) -> Result<MessageOrConversationDependencyFetcher, MailContextError> {
    let mut fetcher = MessageOrConversationDependencyFetcher::new();
    if let Some(conversation_events) = &event.conversations {
        for conversation_event in conversation_events {
            if let Some(conversation) = conversation_event.conversation.as_ref() {
                fetcher.check_conversation(conversation, tether).await?
            }
        }
    }

    if let Some(message_events) = &event.messages {
        for message_event in message_events {
            if let Some(message) = message_event.message.as_ref() {
                fetcher.check_api_message_metadata(message, tether).await?
            }
        }
    }

    Ok(fetcher)
}
