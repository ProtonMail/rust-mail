use crate::event_loop::event_subscriber::CoreEventSubscriberError;
use crate::event_loop::v6::ContactEventSourceV6;
use crate::models::{Contact, Label, ModelExtension};
use crate::{UserContext, join_task};
use anyhow::Context;
use async_trait::async_trait;
use proton_action_queue::action::ActionGroup;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_event_loop::v6::{EventSource, EventSubscriber};
use proton_event_loop::{EventSubscriberError, EventSubscriberResult};
use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};
use std::collections::HashMap;
use std::sync::Weak;
use tracing::{debug, error};

#[derive(Clone)]
pub struct ContactEventV6Subscriber(Weak<UserContext>);

impl From<Weak<UserContext>> for ContactEventV6Subscriber {
    fn from(value: Weak<UserContext>) -> Self {
        Self(value)
    }
}

#[async_trait]
impl EventSubscriber<ContactEventSourceV6> for ContactEventV6Subscriber {
    fn name(&self) -> &'static str {
        "contact-v6-subscriber"
    }

    async fn on_event(
        &self,
        event: &<ContactEventSourceV6 as EventSource>::Event,
        cache: &mut <ContactEventSourceV6 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        let ctx = self
            .0
            .upgrade()
            .context("Context is dead")
            .map_err(CoreEventSubscriberError::Other)
            .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })?;
        async {
            cache.fetch_event_data(event, ctx.session()).await?;

            let mut tether = ctx.user_stash.connection().await?;
            let mut changeset = RebaseChangeSet::default();
            tether
                .tx(async |tx| {
                    if let Some(events) = &event.labels {
                        debug!("Handling contact label event");
                        for event in events {
                            Label::handle_event(
                                tx,
                                &event.id,
                                event.action.into(),
                                cache.get_label_mut(&event.id),
                                &mut changeset,
                            )
                            .await?;
                        }
                    }
                    if let Some(events) = &event.contacts {
                        debug!("Handling contact event");
                        for event in events {
                            Contact::handle_event(
                                tx,
                                &event.id,
                                event.action.into(),
                                cache.get_contact_mut(&event.id),
                                &mut changeset,
                            )
                            .await?;
                        }
                    }

                    if let Err(e) = ctx
                        .rebaseable_queue()
                        .await
                        .rebase_in(ActionGroup::default(), &changeset, tx)
                        .await
                    {
                        error!("Failed to rebase changes: {e}");
                    }
                    Ok::<_, CoreEventSubscriberError>(())
                })
                .await
        }
        .await
        .inspect_err(|e| {
            if !e.is_retryable() {
                ctx.issue_reporter_service().report(
                    IssueLevel::Critical,
                    "Failed to apply contacts (v6) event".into(),
                    issue_report_keys_from_error(e),
                );
            }
        })
        .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }

    async fn on_refresh<'a>(
        &self,
        _: Option<&'a <ContactEventSourceV6 as EventSource>::Event>,
        _: &mut <ContactEventSourceV6 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        let ctx = self
            .0
            .upgrade()
            .context("Context is dead")
            .map_err(CoreEventSubscriberError::Other)
            .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })?;
        if let Err(e) = refresh_contacts(&ctx).await {
            if !e.is_retryable() {
                ctx.issue_reporter_service().report(
                    IssueLevel::Critical,
                    "Failed to apply refresh contacts (v6)".into(),
                    issue_report_keys_from_error(e.as_ref()),
                );
            }
            return Err(e);
        }
        Ok(())
    }
}
#[tracing::instrument(skip_all)]
pub async fn refresh_contacts(ctx: &UserContext) -> EventSubscriberResult<()> {
    async {
        let api = ctx.session().clone();
        let contacts = ctx.spawn(async move { Contact::sync(&api).await });
        let api = ctx.session().clone();
        let all_remote_labels = ctx.spawn(async move { Label::fetch_contact_labels(&api).await });
        let mut tether = ctx.stash().connection().await?;
        let mut all_local_labels: HashMap<_, _> = Label::all_contact_groups(&tether)
            .await?
            .into_iter()
            .map(|label| (label.remote_id.clone(), label))
            .collect();
        debug!(
            "Number of labels available localy: {}",
            all_local_labels.len()
        );
        let all_remote_labels = join_task!(all_remote_labels, "labels");
        debug!(
            "Number of labels available remotely: {}",
            all_remote_labels.len()
        );
        for remote_label in &all_remote_labels {
            all_local_labels.remove(&remote_label.remote_id);
        }

        let contacts = join_task!(contacts, "contacts");

        tether
            .sync_tx(move |tx| {
                Label::store_labels(tx, all_remote_labels).context("Failed to sync labels")?;

                for local_label_to_remove in all_local_labels.into_values() {
                    debug!(
                        "Removing label with remote_id {:?}",
                        local_label_to_remove.remote_id
                    );
                    local_label_to_remove.delete_sync(tx)?;
                }
                contacts.store(tx)?;

                Ok(())
            })
            .await
            .inspect_err(|e| {
                error!("Failed to update database entries while refreshing core: {e}");
            })?;

        Ok::<_, CoreEventSubscriberError>(())
    }
    .await
    .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
}
