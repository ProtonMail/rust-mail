use std::sync::{Arc, Weak};

use crate::{
    CoreContextError, UserContext,
    datatypes::Refresh,
    models::{Address, Contact, Label, User},
};
use anyhow::Context;
use async_trait::async_trait;
use proton_core_api::services::proton::UserId;
use proton_event_loop::{EventLoopError, RefreshFlag};
use stash::{
    orm::Model,
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
use crate::models::LabelError;
pub use macros::*;

use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};

use crate::event_loop::account_event_subscriber::AccountEventSubscriber;
use crate::event_loop::event_source::CoreEventSource;
use crate::event_loop::events::LabelEvent;
use crate::event_loop::v6;
use crate::event_loop::v6::CoreEventCache;
use crate::user_context::event_loop::events::{AddressEvent, ContactEvent, CoreEvent};
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
            CoreEventSubscriberError::Stash(StashError::ConnectionAcquireTimedOut) => true,
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

                ctx.queue()
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

    async fn on_refresh(
        &self,
        refresh_flag: RefreshFlag,
        _: &mut CoreEventCache,
    ) -> EventSubscriberResult<()> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("User context is no longer alive");
            return Ok(());
        };

        ctx.on_refresh_impl(refresh_flag.as_u8().into()).await
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
    pub async fn on_refresh_impl(&self, refresh: Refresh) -> EventSubscriberResult<()> {
        info!("Handling refresh event: {refresh:?}");

        match refresh {
            Refresh::None => {
                warn!("Nothing to refresh, this may idicate bug in SDK event loop implementation");
            }
            Refresh::Contacts => {
                if let Err(e) = v6::refresh_contacts(self).await {
                    if !e.is_retryable() {
                        self.issue_reporter_service().report(
                            IssueLevel::Critical,
                            "Failed to apply refresh contacts (v5)".into(),
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
                if let Err(e) = v6::refresh_core(self).await {
                    if !e.is_retryable() {
                        self.issue_reporter_service().report(
                            IssueLevel::Critical,
                            "Failed to apply refresh (v5)".into(),
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
    pub(crate) async fn register_subscribers(self: &Arc<Self>) -> Result<(), CoreContextError> {
        #[cfg(feature = "events-v6")]
        {
            use anyhow::anyhow;
            let event_loop_service = self.event_loop_service();
            let event_poll = event_loop_service.event_poll();

            let core_event_ctx = v6::CoreEventLoopV6Context::from(Arc::downgrade(self));
            let contact_event_ctx = v6::ContactEventLoopV6Context::from(Arc::downgrade(self));
            event_poll
                .add::<v6::CoreEventSourceV6>(
                    core_event_ctx.clone().boxed(),
                    core_event_ctx.boxed(),
                )
                .await?;
            event_poll
                .add::<v6::ContactEventSourceV6>(
                    contact_event_ctx.clone().boxed(),
                    contact_event_ctx.boxed(),
                )
                .await?;

            event_poll
                .subscribe(self.core_event_subscriber_v6())
                .await?
                .ok_or(CoreContextError::Other(anyhow!(
                    "Failed to register core v6 subscriber"
                )))?;
            event_poll
                .subscribe(self.account_event_subscriber_v6())
                .await?
                .ok_or(CoreContextError::Other(anyhow!(
                    "Failed register account v6 subscriber"
                )))?;
            event_poll
                .subscribe(self.contact_event_subscriber_v6())
                .await?
                .ok_or(CoreContextError::Other(anyhow!(
                    "Failed register contact v6 subscriber"
                )))?;
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

        let result = event_loop_service.event_poll().poll().await;
        if let Err(e) = &result {
            match e {
                EventLoopError::Subscriber(_, e) | EventLoopError::Refresh(_, e)
                    if e.is_retryable() =>
                {
                    // do no report, this error is retryable
                }
                EventLoopError::Provider(e) if e.is_network_failure() => {
                    // do no report, this error is retryable
                }
                e => {
                    self.issue_reporter_service().report(
                        IssueLevel::Critical,
                        "Failed to poll for events".into(),
                        issue_report_keys_from_error(e),
                    );
                }
            }
        }

        result
    }

    #[must_use]
    pub fn event_subscriber(self: &Arc<Self>) -> impl EventSubscriber<CoreEventSource> + 'static {
        CoreEventSubscriber::from(Arc::downgrade(self))
    }

    #[must_use]
    pub fn core_event_subscriber_v6(
        self: &Arc<Self>,
    ) -> impl EventSubscriber<v6::CoreEventSourceV6> + 'static {
        v6::CoreEventV6Subscriber::from(Arc::downgrade(self))
    }

    #[must_use]
    pub fn contact_event_subscriber_v6(
        self: &Arc<Self>,
    ) -> impl EventSubscriber<v6::ContactEventSourceV6> + 'static {
        v6::ContactEventV6Subscriber::from(Arc::downgrade(self))
    }

    #[must_use]
    pub fn account_event_subscriber_v6(
        self: &Arc<Self>,
    ) -> impl EventSubscriber<v6::CoreEventSourceV6> + 'static {
        v6::AccountEventV6Subscriber::from(Arc::downgrade(self))
    }

    #[must_use]
    pub fn account_event_subscriber(
        self: &Arc<Self>,
    ) -> impl EventSubscriber<CoreEventSource> + 'static {
        AccountEventSubscriber::from(Arc::downgrade(self))
    }
}

async fn handle_address_event(
    tx: &Bond<'_>,
    address_events: &mut [AddressEvent],
    changeset: &mut RebaseChangeSet,
) -> Result<(), StashError> {
    for event in address_events {
        Address::handle_event(
            tx,
            &event.remote_id,
            event.action,
            event.address.as_mut(),
            changeset,
        )
        .await?;
    }

    Ok(())
}

async fn handle_contact_event(
    tx: &Bond<'_>,
    contact_events: &mut [ContactEvent],
    changeset: &mut RebaseChangeSet,
) -> Result<(), StashError> {
    for event in contact_events {
        Contact::handle_event(
            tx,
            &event.remote_id,
            event.action,
            event.contact.as_mut(),
            changeset,
        )
        .await?;
    }
    Ok(())
}

pub async fn handle_label_events(
    tx: &Bond<'_>,
    label_events: &mut [LabelEvent],
    changeset: &mut RebaseChangeSet,
) -> Result<(), StashError> {
    for label_event in label_events {
        Label::handle_event(
            tx,
            &label_event.remote_id,
            label_event.action,
            label_event.label.as_mut(),
            changeset,
        )
        .await?;
    }
    Ok(())
}
