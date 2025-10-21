use crate::actions::refresh::ActionRefresh;
use crate::actions::{conversations, messages};
use crate::datatypes::{LocalConversationId, LocalMessageId};
use crate::datatypes::{MessageLabelsCount, ReadFilter, ViewMode};
use crate::models::{
    CachedScrollData, ConversationScrollData, IncomingDefault, MailLabel, MailSettings,
    MessageScrollData, RollbackItem, StoreLabelCounters,
};
#[cfg(feature = "prefetch")]
use crate::prefetch::PrefetchJob;
use crate::user_context::events::conversations::handle_conversation_events;
use crate::user_context::events::labels::handle_label_events;
use crate::user_context::events::messages::handle_message_events;
use crate::{MailContextError, MailUserContext};
use crate::{datatypes::ConversationLabelsCount, events::MailEvent};
use anyhow::Context;
use anyhow::anyhow;
use async_trait::async_trait;
use indoc::formatdoc;
use proton_action_queue::queue::{ActionError as QueueActionError, QueuedActionOutput};
use proton_core_common::datatypes::{Refresh, SystemLabel};
use proton_core_common::models::Label;
use proton_event_loop::subscriber::{Subscriber, SubscriberError};
use stash::orm::Model;
use std::collections::HashMap;
use std::sync::Weak;
use tracing::{debug, error, info, warn};

// Import common macros from core
use crate::datatypes::dependencies::MessageOrConversationDependencyFetcher;
use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
use proton_core_common::event_loop::{join_task, try_refresh};
use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};
use stash::stash::Tether;

pub struct MailEventSubscriber(Weak<MailUserContext>);

impl MailEventSubscriber {
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

#[async_trait]
impl Subscriber<MailEvent> for MailEventSubscriber {
    fn name(&self) -> &'static str {
        "proton-mail-event-subscriber"
    }

    async fn on_events(&self, events: &mut [MailEvent]) -> Result<(), SubscriberError> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("Mail user context is no longer alive");
            return Ok(());
        };

        debug!("Handling {} mail events", events.len());

        let mut tether = ctx.user_context.stash().connection().await?;
        let mut data = PostEventSyncData::default();

        // Check for missing dependencies. Sometimes when lot of messages/conversations get moved
        // to newly created label the items can have the new label before the label create event.

        calculate_missing_dependencies(events, &tether)
            .await
            .context("Failed to calculate dependencies")?
            .fetch_and_store(ctx.session(), &mut tether)
            .await
            .context("Failed to fetch or store dependencies")?;

        tether
            .tx::<_, _, SubscriberError>(async |tx| {
                for event in events {
                    if let Some(labels) = &event.labels {
                        debug!("Handling label events");
                        handle_label_events(tx, labels)
                            .await
                            .context("Error handling label events")?;
                    }

                    if let Some(conversations) = &event.conversations {
                        debug!("Handling conversation events");
                        handle_conversation_events(tx, conversations, &mut data)
                            .await
                            .context("Error handling conversation events")?;
                    }

                    if let Some(messages) = &event.messages {
                        debug!("Handling message events");
                        handle_message_events(tx, messages, &mut data)
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
                    data.queue_incoming_default |= event.incoming_defaults.is_some();
                }
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

        #[cfg(feature = "prefetch")]
        {
            let label_id = SystemLabel::AllMail.local_id(&tether).await?.unwrap();
            let conversation_jobs = data
                .cnv_for_prefetch
                .into_iter()
                .map(|id| PrefetchJob::Conversation(id, label_id))
                .collect();
            let message_jobs = data
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
        }

        if data.queue_incoming_default {
            IncomingDefault::action_resync(ctx.action_queue()).await;
        }

        Ok(())
    }

    async fn on_refresh(&self, event: &MailEvent) -> Result<(), SubscriberError> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("Mail user context is no longer alive");
            return Ok(());
        };
        ctx.on_refresh_impl(event.refresh).await
    }

    fn is_alive(&self) -> bool {
        self.0.strong_count() > 0
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

    pub async fn on_refresh_impl(&self, refresh: Refresh) -> Result<(), SubscriberError> {
        info!("Handling refresh event: {refresh:?}");

        match refresh {
            Refresh::None => {
                warn!("Nothing to refresh, this may idicate bug in SDK event loop implementation");
            }
            Refresh::Mail | Refresh::All => try_refresh!(refresh_mail, self),
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

#[tracing::instrument(skip_all)]
async fn refresh_mail(ctx: &MailUserContext) -> Result<(), SubscriberError> {
    let api = ctx.session().clone();
    let all_remote_labels = ctx.spawn(async move { Label::fetch_mail_labels(&api).await });
    let api = ctx.session().clone();
    let counters = ctx.spawn(async move { StoreLabelCounters::fetch(&api).await });
    let api = ctx.session().clone();
    let mail_settings = ctx.spawn(async move { MailSettings::sync_mail_settings(&api).await });

    let mut tether = ctx.user_context.stash().connection().await?;
    let mut all_local_labels: HashMap<_, _> = Label::all_mail(&tether)
        .await?
        .into_iter()
        .map(|label| (label.remote_id.clone(), label))
        .collect();
    debug!(
        "Number of labels available localy: {}",
        all_local_labels.len()
    );

    let all_remote_labels = join_task!(all_remote_labels, "labels");
    let counters = join_task!(counters, "label counters");
    let mail_settings = join_task!(mail_settings, "mail settings");

    debug!(
        "Number of labels available remotely: {}",
        all_remote_labels.len()
    );
    for remote_label in all_remote_labels.iter() {
        all_local_labels.remove(&remote_label.remote_id);
    }

    tether
        .sync_tx(move |tx| {
            tx.execute_batch(&formatdoc! {"
                DELETE from {};
                DELETE from {};
                DELETE from {};
                ",
                RollbackItem::table_name(),
                ConversationScrollData::table_name(),
                MessageScrollData::table_name(),
            })?;

            Label::store_labels(tx, all_remote_labels).context("Failed to sync labels")?;

            let mut ids = vec![];
            for local_label_to_remove in all_local_labels.into_values() {
                if let Some(_system_label) =
                    SystemLabel::from_opt_rid(local_label_to_remove.remote_id.as_ref())
                {
                    // For some reason API does not return all system labels
                    // we have to make sure to not delete those
                    continue;
                }

                debug!(
                    "Removing label with remote_id {:?}",
                    local_label_to_remove.remote_id
                );
                ids.push(local_label_to_remove.id());
            }
            counters.store(tx)?;
            mail_settings.store(tx)?;

            Ok(())
        })
        .await
        .inspect_err(|e| {
            error!("Failed to clear database entries, while refreshing mail: {e}");
        })?;

    IncomingDefault::action_resync(ctx.action_queue()).await;

    let all_mail = SystemLabel::AllMail
        .load(&tether)
        .await?
        .ok_or_else(|| anyhow!("All mail label is missing!"))?;
    let page_size = 50; // 80 exceeds HTTP URI limit, 50 seems to be safe softspot

    match all_mail.view_mode(&tether).await? {
        ViewMode::Conversations => {
            let mut conv_scroll_cursor = CachedScrollData::<ConversationScrollData>::all(
                all_mail.id(),
                ReadFilter::All,
                page_size,
                ScrollOrderDir::default(),
                ScrollOrderField::default(),
            );

            info!(
                "Queue conversations to refresh, count: {}",
                conv_scroll_cursor.synced_count(&tether).await?
            );

            while let Some(page) = conv_scroll_cursor.while_fetch_more(&tether).await? {
                let local_conv_ids = page.into_iter().map(|conv| conv.local_id).collect();

                let action = conversations::RefreshMetadata::new(local_conv_ids);
                if let Err(error) = ctx.action_queue().queue_action(action).await {
                    error!("Failed to refresh conversation metadata: `{error}`",);
                }
            }
        }
        ViewMode::Messages => {
            let mut msg_scroll_cursor = CachedScrollData::<MessageScrollData>::all(
                all_mail.id(),
                ReadFilter::All,
                page_size,
                ScrollOrderDir::default(),
                ScrollOrderField::default(),
            );

            info!(
                "Queue messages to refresh, count: {}",
                msg_scroll_cursor.synced_count(&tether).await?
            );

            while let Some(page) = msg_scroll_cursor.while_fetch_more(&tether).await? {
                let local_msg_ids = page.into_iter().filter_map(|msg| msg.local_id).collect();
                let action = messages::RefreshMetadata::new(local_msg_ids);

                if let Err(error) = ctx.action_queue().queue_action(action).await {
                    error!("Failed to refresh message metadata: `{error}`",);
                }
            }
        }
    };
    Ok(())
}

async fn calculate_missing_dependencies(
    events: &[MailEvent],
    tether: &Tether,
) -> Result<MessageOrConversationDependencyFetcher, MailContextError> {
    let mut fetcher = MessageOrConversationDependencyFetcher::new();
    for event in events {
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
    }

    Ok(fetcher)
}
