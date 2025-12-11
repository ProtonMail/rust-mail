use crate::MailUserContext;
use crate::actions::{conversations, messages};
use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
use crate::datatypes::{ConversationLabelsCount, MessageLabelsCount, ReadFilter, ViewMode};
use crate::events::event_subscriber::{MailEventSubscriberError, PostEventSyncData};
use crate::events::labels::handle_counters_label_event;
use crate::events::v6::MailEventSourceV6;
use crate::models::{
    CachedScrollData, Conversation, ConversationScrollData, IncomingDefault, MailLabel,
    MailSettings, Message, MessageScrollData, RollbackItem, StoreLabelCounters,
};
use anyhow::{Context, anyhow};
use async_trait::async_trait;
use indoc::formatdoc;
use itertools::Itertools;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::join_task;
use proton_core_common::models::Label;
use proton_event_loop::v6::{EventSource, EventSubscriber};
use proton_event_loop::{EventSubscriberError, EventSubscriberResult};
use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};
use stash::orm::Model;
use std::collections::HashMap;
use std::sync::Weak;
use tracing::{debug, error, info, warn};

pub struct MailEventV6Subscriber(Weak<MailUserContext>);

impl From<Weak<MailUserContext>> for MailEventV6Subscriber {
    fn from(value: Weak<MailUserContext>) -> Self {
        Self(value)
    }
}

#[async_trait]
impl EventSubscriber<MailEventSourceV6> for MailEventV6Subscriber {
    fn name(&self) -> &'static str {
        "mail-v6-subscriber"
    }

    async fn on_event(
        &self,
        event: &<MailEventSourceV6 as EventSource>::Event,
        cache: &mut <MailEventSourceV6 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("Mail user context is no longer alive");
            return Ok(());
        };

        async {
            cache.fetch_event_data(ctx.session(), event).await?;
            let mut tether = ctx.user_stash().connection().await?;

            //TODO: missing dependencies should check the cache?
            cache
                .calculate_missing_dependencies(&tether)
                .await?
                .fetch_and_store(ctx.session(), &mut tether)
                .await
                .context("Failed to fetch or store dependencies")?;

            let mut changeset = RebaseChangeSet::default();
            let mut post_event_data = PostEventSyncData::default();
            tether
                .tx(async |tx| {
                    if let Some(mail_setting) = cache.get_settings_mut() {
                        debug!("Handling mail settings event");
                        mail_setting.save(tx).await?;
                    }

                    if let Some(ref events) = event.labels {
                        debug!("Handling label events");
                        for event in events {
                            let action = event.action.into();
                            Label::handle_event(
                                tx,
                                &event.id,
                                action,
                                cache.get_label_mut(&event.id),
                                &mut changeset,
                            )
                            .await?;
                            handle_counters_label_event(tx, &event.id, action).await?;
                        }
                    }

                    if let Some(ref events) = event.conversations {
                        for event in events {
                            debug!("Handling conversation events");
                            if let Some(id) = Conversation::handle_event(
                                tx,
                                &event.id,
                                event.action.into(),
                                cache.get_conversation_mut(&event.id),
                                &mut changeset,
                            )
                            .await?
                            {
                                post_event_data.cnv_for_prefetch.push(id);
                            }
                        }
                    }

                    if let Some(ref events) = event.messages {
                        for event in events {
                            debug!("Handling message events");
                            if let Some(id) = Message::handle_event(
                                tx,
                                &event.id,
                                event.action.into(),
                                cache.get_message(&event.id),
                                &mut changeset,
                            )
                            .await?
                            {
                                post_event_data.msg_for_prefetch.push(id);
                            }
                        }
                    }

                    let conversation_counts = cache
                        .get_conversation_counts()
                        .cloned()
                        .map_into()
                        .collect::<Vec<_>>();
                    if !conversation_counts.is_empty() {
                        tracing::debug!("Handling conversation counts");
                        ConversationLabelsCount::create_or_update_conversation_counts(
                            conversation_counts,
                            tx,
                        )
                        .await?;
                    }

                    let message_counts = cache
                        .get_message_counts()
                        .cloned()
                        .map_into()
                        .collect::<Vec<_>>();
                    if !message_counts.is_empty() {
                        tracing::debug!("Handling message counts");
                        MessageLabelsCount::create_or_update_message_counts(message_counts, tx)
                            .await?;
                    }

                    if event
                        .incoming_defaults
                        .as_ref()
                        .is_some_and(|e| !e.is_empty())
                    {
                        post_event_data.update_incoming_default();
                    }

                    Ok::<_, MailEventSubscriberError>(())
                })
                .await?;

            post_event_data.apply(&ctx, &tether).await?;

            Ok::<_, MailEventSubscriberError>(())
        }
        .await
        .inspect_err(|e| {
            ctx.issue_reporter_service().report(
                IssueLevel::Error,
                "Failed to apply mail event v6".into(),
                issue_report_keys_from_error(e),
            )
        })
        .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }

    async fn on_refresh<'a>(
        &self,
        _: Option<&'a <MailEventSourceV6 as EventSource>::Event>,
        _: &mut <MailEventSourceV6 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("Mail user context is no longer alive");
            return Ok(());
        };
        refresh_mail(&ctx)
            .await
            .inspect_err(|e| {
                ctx.issue_reporter_service().report(
                    IssueLevel::Error,
                    "Failed to apply refresh event v6".into(),
                    issue_report_keys_from_error(e),
                )
            })
            .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }
}

#[tracing::instrument(skip_all)]
pub async fn refresh_mail(ctx: &MailUserContext) -> Result<(), MailEventSubscriberError> {
    let api = ctx.session().clone();
    let all_remote_labels = ctx.spawn(async move { Label::fetch_mail_labels(&api).await });
    let api = ctx.session().clone();
    let counters = ctx.spawn(async move { StoreLabelCounters::fetch(&api).await });
    let api = ctx.session().clone();
    let mail_settings = ctx.spawn(async move { MailSettings::fetch_mail_settings(&api).await });

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
    Ok::<_, MailEventSubscriberError>(())
}
