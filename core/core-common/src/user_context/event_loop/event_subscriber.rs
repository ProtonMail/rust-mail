use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use crate::{
    CoreContextError, UserContext,
    datatypes::Refresh,
    models::{Address, Contact, Label, ModelExtension, User},
};
use anyhow::Context;
use async_trait::async_trait;
use proton_core_api::services::proton::UserId;
use proton_event_loop::EventLoopError;
use stash::{
    orm::Model,
    params,
    stash::{Bond, StashError},
};
use tracing::{debug, error, info, warn};

pub mod macros {
    #[macro_export]
    macro_rules! join_task {
        ($name:tt, $description: expr) => {{
            match $name.await {
                Ok(Ok(value)) => value,

                Ok(Err(err)) => return Err(err.into()),

                Err(err) => {
                    return if err.is_cancelled() {
                        Err(anyhow::anyhow!(
                            "The task `{}` was cancelled, we need to run refresh again",
                            $description
                        )
                        .into())
                    } else {
                        Err(anyhow::anyhow!(
                            "Failed to join download remote {}: `{err}`",
                            $description
                        )
                        .into())
                    };
                }
            }
        }};
    }

    pub use join_task;
}

// Re-export macros for easier access
use crate::models::{LabelError, ModelIdExtension};
pub use macros::*;

use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};

use crate::event_loop::account_event_subscriber::AccountEventSubscriber;
use crate::event_loop::event_source::{CoreEventCache, CoreEventSource};
use crate::event_loop::events::LabelEvent;
use crate::user_context::event_loop::events::{Action, AddressEvent, ContactEvent, CoreEvent};
use proton_action_queue::action::ActionGroup;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::service::ApiServiceError;
use proton_event_loop::v6::{EventSource, EventSubscriberResult};
use proton_event_loop::v6::{EventSubscriber, EventSubscriberError};

#[derive(Debug, thiserror::Error)]
pub enum CoreEventSubscriberError {
    #[error(transparent)]
    Api(#[from] ApiServiceError),
    #[error(transparent)]
    Stash(#[from] StashError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<CoreContextError> for CoreEventSubscriberError {
    fn from(err: CoreContextError) -> Self {
        match err {
            CoreContextError::Api(e) => Self::Api(e),
            CoreContextError::Stash(e) => Self::Stash(e),
            e => CoreEventSubscriberError::Other(e.into()),
        }
    }
}

impl From<LabelError> for CoreEventSubscriberError {
    fn from(err: LabelError) -> Self {
        match err {
            LabelError::API(e) => Self::Api(e),
            LabelError::Stash(e) => Self::Stash(e),
            err => CoreEventSubscriberError::Other(err.into()),
        }
    }
}

impl EventSubscriberError for CoreEventSubscriberError {
    fn is_network_failure(&self) -> bool {
        match self {
            CoreEventSubscriberError::Api(e) => e.is_network_failure(),
            CoreEventSubscriberError::Stash(_) | CoreEventSubscriberError::Other(_) => false,
        }
    }

    fn is_retryable(&self) -> bool {
        match self {
            CoreEventSubscriberError::Api(e) => e.is_network_failure() || e.is_server_failure(),
            CoreEventSubscriberError::Stash(_) | CoreEventSubscriberError::Other(_) => false,
        }
    }
}

/// Event loop subscriber for core events
#[derive(Clone)]
pub struct CoreEventSubscriber(Weak<UserContext>);

impl CoreEventSubscriber {
    #[must_use]
    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }
}

impl From<Weak<UserContext>> for CoreEventSubscriber {
    fn from(value: Weak<UserContext>) -> Self {
        Self(value)
    }
}

#[async_trait]
impl EventSubscriber<CoreEventSource> for CoreEventSubscriber {
    fn name(&self) -> &'static str {
        "core-event-subscriber"
    }

    #[tracing::instrument(skip_all)]
    async fn on_event(
        &self,
        event: &<CoreEventSource as EventSource>::Event,
        _: &mut CoreEventCache,
    ) -> EventSubscriberResult<()> {
        async {
            let Some(ctx) = self.0.upgrade() else {
                warn!("User context is no longer alive");
                return Ok(());
            };
            let user_id = ctx.user_id().clone();
            let stash = ctx.stash().clone();
            let mut conn = stash.connection().await?;

            let mut rebase_change_set = RebaseChangeSet::default();

            conn.tx::<_, _, StashError>(async |tx| {
                handle_event(event, tx, &user_id, &mut rebase_change_set).await?;

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
                    IssueLevel::Critical,
                    "Failed to apply core event".into(),
                    issue_report_keys_from_error(e),
                );
            })
            .context("Failed to apply event")
            .map_err(CoreEventSubscriberError::Other)
        }
        .await
        .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }

    async fn on_refresh<'a>(
        &self,
        event: Option<&'a <CoreEventSource as EventSource>::Event>,
        cache: &mut CoreEventCache,
    ) -> EventSubscriberResult<()> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("User context is no longer alive");
            return Ok(());
        };

        ctx.on_refresh_impl(
            event.map_or(Refresh::All, |event| event.refresh.into()),
            cache,
        )
        .await
    }
}

async fn handle_event(
    event: &<CoreEventSource as EventSource>::Event,
    tx: &Bond<'_>,
    user_id: &UserId,
    rebase_change_set: &mut RebaseChangeSet,
) -> Result<(), StashError> {
    // To support current v5 model, we make a clone of the data here and cast it back to the
    // crate's data type. In v6 this can be performed after the data is fetched from the server.
    let mut event: CoreEvent = event.clone().into();
    if let Some(user) = event.user.as_mut() {
        debug!("Handling user event");
        // Update user:
        user.save(tx).await.map_err(|e| {
            error!("Failed to update user: {e:?}");
            e
        })?;
    }
    if let Some(settings) = event.user_settings.as_mut() {
        debug!("Handling user setting event");
        settings.remote_id = Some(user_id.clone());
        settings.save(tx).await.map_err(|e| {
            error!("Failed to update user settings:{e:?}");
            e
        })?;
    }
    if let Some(used_space) = event.used_space {
        debug!("Handling user space event");
        let mut user = User::load(user_id.clone(), tx).await?.unwrap();
        user.used_space = used_space;
        user.save(tx).await.map_err(|e| {
            error!("Failed to update used space:{e:?}");
            e
        })?;
    }
    if let Some(used_product_space) = event.product_used_space.as_ref() {
        debug!("Handling user product space event");
        let mut user = User::load(user_id.clone(), tx).await?.unwrap();
        user.product_used_space = used_product_space.clone();
        user.save(tx).await.map_err(|e| {
            error!("Failed to update used space:{e:?}");
            e
        })?;
    }
    if let Some(addresses) = event.addresses.as_mut() {
        debug!("Handling address event");
        handle_address_event(tx, addresses, rebase_change_set).await?;
    }

    if let Some(labels) = event.labels.as_mut() {
        debug!("Handling label event");
        handle_label_events(tx, labels, rebase_change_set).await?;
    }

    // We don't need to handle contact email events as teh contact event includes
    // the full state with vcards and the emails.
    if let Some(contacts) = event.contacts.as_mut() {
        debug!("Handling contact events");
        handle_contact_event(tx, contacts, rebase_change_set).await?;
    }

    Ok(())
}

impl UserContext {
    pub async fn on_refresh_impl(
        &self,
        refresh: Refresh,
        cache: &mut CoreEventCache,
    ) -> EventSubscriberResult<()> {
        info!("Handling refresh event: {refresh:?}");

        match refresh {
            Refresh::None => {
                warn!("Nothing to refresh, this may idicate bug in SDK event loop implementation");
            }
            Refresh::Contacts => {
                if let Err(e) = refresh_contacts(self).await {
                    if !e.is_retryable() {
                        self.issue_reporter_service().report(
                            IssueLevel::Critical,
                            "Failed to apply refresh contacts".into(),
                            issue_report_keys_from_error(e.as_ref()),
                        );
                    }
                    return Err(e);
                }
            }
            Refresh::Mail => {
                // Mail refresh is handled by the mail context
            }
            Refresh::All => {
                if let Err(e) = refresh_core(self, cache).await {
                    if !e.is_retryable() {
                        self.issue_reporter_service().report(
                            IssueLevel::Critical,
                            "Failed to apply refresh".into(),
                            issue_report_keys_from_error(e.as_ref()),
                        );
                    }
                    return Err(e);
                }
            }
            Refresh::Unknown(other) => {
                warn!("Unknown refresh event type: {other}");
            }
        }

        Ok(())
    }

    /// Register the core event subscriber.
    ///
    /// Whether there is a need to add a new subscriber to `CoreEvents` it should
    /// be done here. Example how to add a new subscriber:
    ///
    /// ```ignore
    /// let core_subscriber = CoreEventSubscriber::from(Arc::downgrade(self));
    /// let new_core_subscriber = NewCoreEventSubscriber::from(Arc::downgrade(self));
    /// let mut core_subscribers = TypedSubscribers::<CoreEvent>::from(core_subscriber.boxed());
    /// core_subscribers.add_subscriber(new_core_subscriber);
    ///
    /// self.event_loop.register(core_subscribers).await?;
    /// ```
    ///
    ///
    #[cfg_attr(
        not(feature = "events-v6"),
        allow(clippy::unused_async, reason = "Temporary progression")
    )]
    pub(crate) async fn register_subscribers(self: &Arc<Self>) -> Result<(), EventLoopError> {
        #[cfg(feature = "events-v6")]
        {
            todo!("Setup v6 event source and subscriber");
            let event_loop_service = self.event_loop_service();

            let event_poll = event_loop_service.event_poll();
            event_poll.subscribe(self.event_subscriber()).await?;
            event_poll
                .subscribe(self.account_event_subscriber())
                .await?;
        }

        Ok(())
    }

    /// Perform one iteration of the event loop, which consists of retrieving the latest events,
    /// publishing it on all the registered subscribers and storing the event id for the next
    /// iteration.
    ///
    /// The execution of the polling is aborted on the first error.
    ///
    pub async fn poll_event_loop_impl(&self) -> Result<(), EventLoopError> {
        let event_loop_service = self.event_loop_service();

        event_loop_service.event_poll().poll().await
    }

    #[must_use]
    pub fn event_subscriber(self: &Arc<Self>) -> impl EventSubscriber<CoreEventSource> + 'static {
        CoreEventSubscriber::from(Arc::downgrade(self))
    }

    #[must_use]
    pub fn account_event_subscriber(
        self: &Arc<Self>,
    ) -> impl EventSubscriber<CoreEventSource> + 'static {
        AccountEventSubscriber::from(Arc::downgrade(self))
    }
}

#[tracing::instrument(skip_all)]
async fn refresh_core(ctx: &UserContext, _: &mut CoreEventCache) -> EventSubscriberResult<()> {
    async {
        let api = ctx.session().clone();
        let contacts = ctx.spawn(async move { Contact::sync(&api).await });
        let api = ctx.session().clone();
        let all_remote_addresses = ctx.spawn(async move { Address::sync(&api).await });
        let api = ctx.session().clone();
        let user_and_settings = ctx.spawn(async move { User::sync_user_and_settings(&api).await });
        let api = ctx.session().clone();
        let all_remote_labels = ctx.spawn(async move { Label::fetch_contact_labels(&api).await });

        let mut tether = ctx.stash().connection().await?;
        let mut all_local_addresses: HashMap<_, _> = Address::all(&tether)
            .await?
            .into_iter()
            .map(|addr| (addr.remote_id.clone(), addr))
            .collect();
        let mut all_local_labels: HashMap<_, _> = Label::all_contact_groups(&tether)
            .await?
            .into_iter()
            .map(|label| (label.remote_id.clone(), label))
            .collect();
        debug!(
            "Number of labels available localy: {}",
            all_local_labels.len()
        );

        debug!(
            "Number of addresses available localy: {}",
            all_local_addresses.len()
        );

        let all_remote_addresses = join_task!(all_remote_addresses, "addresses").inner();
        let user_and_settings = join_task!(user_and_settings, "user and settings");
        let all_remote_labels = join_task!(all_remote_labels, "labels");

        debug!(
            "Number of addresses available remotely: {}",
            all_remote_addresses.len()
        );
        for remote_label in &all_remote_addresses {
            all_local_addresses.remove(&remote_label.remote_id);
        }
        debug!(
            "Number of labels available remotely: {}",
            all_remote_labels.len()
        );
        for remote_label in &all_remote_labels {
            all_local_labels.remove(&remote_label.remote_id);
        }

        let contacts = join_task!(contacts, "contacts");

        tether
            .tx::<_, _, CoreEventSubscriberError>(async |tx| {
                for local_address_to_remove in all_local_addresses.into_values() {
                    debug!(
                        "Removing address with remote_id {:?}",
                        local_address_to_remove.remote_id
                    );
                    local_address_to_remove.delete(tx).await?;
                }
                for mut remote_address in all_remote_addresses {
                    remote_address.save(tx).await?;
                }

                Label::store_labels_async(tx, all_remote_labels)
                    .await
                    .map_err(|e| anyhow::Error::new(e).context("Failed to store labels"))?;

                for local_label_to_remove in all_local_labels.into_values() {
                    debug!(
                        "Removing label with remote_id {:?}",
                        local_label_to_remove.remote_id
                    );
                    local_label_to_remove.delete(tx).await?;
                }

                tx.sync_bridge(move |tx| {
                    user_and_settings.store(tx)?;
                    contacts.store(tx)?;
                    Ok(())
                })
                .await?;

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

#[tracing::instrument(skip_all)]
async fn refresh_contacts(ctx: &UserContext) -> EventSubscriberResult<()> {
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

async fn handle_address_event(
    tx: &Bond<'_>,
    address_events: &mut [AddressEvent],
    rebase_change_set: &mut RebaseChangeSet,
) -> Result<(), StashError> {
    for event in address_events {
        event
            .action
            .log_entry(&event.remote_id, async |remote_id| {
                Address::remote_id_counterpart(remote_id.clone(), tx)
                    .await
                    .unwrap_or_default()
                    .map(|v| v.as_u64())
            })
            .await;
        match event.action {
            Action::Delete => {
                warn!("[ET-1461] Delete action not implemented for address event");
            }

            Action::Create | Action::Update => {
                if let Some(ref mut address) = event.address {
                    address.save(tx).await?;
                    rebase_change_set.add(address.id());
                }
            }

            Action::UpdateFlags => {
                warn!("[ET-1461] UpdateFlags action not implemented for address event");
            }
        }
    }

    Ok(())
}

async fn handle_contact_event(
    tx: &Bond<'_>,
    contact_events: &mut [ContactEvent],
    rebase_change_set: &mut RebaseChangeSet,
) -> Result<(), StashError> {
    for event in contact_events {
        event
            .action
            .log_entry(&event.remote_id, async |remote_id| {
                Contact::remote_id_counterpart(remote_id.clone(), tx)
                    .await
                    .unwrap_or_default()
                    .map(|v| v.as_u64())
            })
            .await;
        match event.action {
            Action::Delete => tx
                .execute(
                    "DELETE FROM contacts WHERE remote_id = ?",
                    params![event.remote_id.clone()],
                )
                .await
                .map(|_| ())
                .map_err(|e| {
                    error!("Failed to delete contact: {e:?}");
                    e
                })?,
            Action::Create | Action::Update => {
                if let Some(ref mut contact) = event.contact {
                    contact.save(tx).await.map_err(|e| {
                        error!("Failed to create or update contact: {e:?}");
                        e
                    })?;
                    rebase_change_set.add(contact.id());
                }
            }
            Action::UpdateFlags => (),
        }
    }
    Ok(())
}

pub async fn handle_label_events(
    tx: &Bond<'_>,
    label_events: &[LabelEvent],
    rebase_change_set: &mut RebaseChangeSet,
) -> Result<(), StashError> {
    for label_event in label_events {
        label_event
            .action
            .log_entry(&label_event.remote_id, async |remote_id| {
                Label::remote_id_counterpart(remote_id.clone(), tx)
                    .await
                    .unwrap_or_default()
                    .map(|v| v.as_u64())
            })
            .await;
        match label_event.action {
            Action::Delete => {
                tx.execute(
                    "DELETE FROM labels WHERE remote_id = ?",
                    params![label_event.remote_id.clone()],
                )
                .await?;
            }
            Action::Create => {
                if let Some(mut label) = label_event.label.clone() {
                    label.save(tx).await?;
                    rebase_change_set.add(label.id());
                } else {
                    warn!("Received label create without label");
                }
            }
            Action::Update | Action::UpdateFlags => {
                if let Some(mut label) = label_event.label.clone() {
                    label.save(tx).await?;
                    rebase_change_set.add(label.id());
                } else {
                    warn!("Received label update without label");
                }
            }
        }
    }
    Ok(())
}
