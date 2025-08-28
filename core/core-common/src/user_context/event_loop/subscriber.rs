use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use crate::{
    UserContext,
    datatypes::Refresh,
    db::account::CoreAccount,
    events::{Action, AddressEvent, ContactEmailEvent, ContactEvent, CoreEvent},
    models::{Address, Contact, Label, ModelExtension, User},
};
use anyhow::{anyhow, bail};
use async_trait::async_trait;
use proton_core_api::services::proton::{EventId, ProtonCore, UserId};
use proton_event_loop::{
    EventLoopError, RawEvent,
    provider::Provider,
    store::Store,
    subscriber::{Subscriber, SubscriberError},
};
use stash::{
    exports::SqliteError,
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

                Ok(Err(err)) => {
                    return Err(anyhow::anyhow!(
                        "Failed to download remote {}: `{err}`",
                        $description
                    )
                    .into());
                }

                Err(err) => {
                    return if err.is_cancelled() {
                        Err(proton_event_loop::subscriber::SubscriberError::Other(
                            anyhow::anyhow!(
                                "The task `{}` was cancelled, we need to run refresh again",
                                $description
                            ),
                        ))
                    } else {
                        Err(
                            anyhow::anyhow!("Failed to download remote {}: `{err}`", $description)
                                .into(),
                        )
                    };
                }
            }
        }};
    }

    #[macro_export]
    macro_rules! try_refresh {
        ($fn_name:tt, $ctx:expr) => {{
            let max_attempts = 2;
            let mut attempts = 0;

            while let Err(e) = $fn_name($ctx).await {
                if attempts >= max_attempts {
                    return Err(e);
                }
                attempts += 1;
                tracing::warn!("Refresh event attempt {attempts} failed: `{e}`");
            }
        }};
    }

    pub use join_task;
    pub use try_refresh;
}

// Re-export macros for easier access
use crate::events::LabelEvent;
pub use macros::*;

const CORE_EVENT_TYPE_ID: &str = "proton-core-event";

/// Event loop context for core events
#[derive(Clone)]
pub struct CoreEventLoopContext(Weak<UserContext>);

impl CoreEventLoopContext {
    pub fn inner(&self) -> Result<Arc<UserContext>, anyhow::Error> {
        match self.0.upgrade() {
            Some(ctx) => Ok(ctx),
            None => bail!("UserContext no longer alive"),
        }
    }

    #[must_use]
    pub fn boxed(&self) -> Box<Self> {
        Box::new(self.clone())
    }
}

impl From<Weak<UserContext>> for CoreEventLoopContext {
    fn from(value: Weak<UserContext>) -> Self {
        Self(value)
    }
}

#[async_trait]
impl Store for CoreEventLoopContext {
    async fn load(&self) -> anyhow::Result<Option<EventId>> {
        let ctx = self.inner()?;
        let tether = ctx.stash().connection();
        match tether
            .query_value::<_, EventId>(
                "SELECT value FROM event_id_store WHERE id = ?1",
                params![CORE_EVENT_TYPE_ID],
            )
            .await
        {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                if matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    Ok(None)
                } else {
                    error!("Failed to load core event id from db:{e:?}");
                    Err(anyhow!("Failed to load core event id {e}"))
                }
            }
        }
    }

    async fn store(&self, id: EventId) -> anyhow::Result<()> {
        let ctx = self.inner()?;
        ctx.stash()
            .connection()
            .tx(async |tx| {
                tx.execute(
                    "INSERT OR REPLACE INTO event_id_store (id, value) VALUES (?, ?)",
                    params![CORE_EVENT_TYPE_ID, id],
                )
                .await?;

                Ok(())
            })
            .await
            .map_err(|e: StashError| {
                error!("Failed to store core event id in db:{e:?}");
                anyhow!("Failed to store core event id {e}")
            })
    }
}

#[async_trait]
impl Provider for CoreEventLoopContext {
    async fn get_latest_event_id(
        &self,
    ) -> Result<EventId, proton_core_api::service::ApiServiceError> {
        let ctx = self.inner()?;
        Ok(ctx.session().get_events_latest().await?.event_id)
    }

    async fn get_event(
        &self,
        event_id: &EventId,
    ) -> Result<RawEvent, proton_core_api::service::ApiServiceError> {
        let ctx = self.inner()?;
        let json_string = ctx
            .session()
            .get_event(
                event_id.clone(),
                proton_core_api::services::proton::GetEventOptions::all(),
            )
            .await?;

        Ok(RawEvent::from_json(json_string)?)
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
impl Subscriber<CoreEvent> for CoreEventSubscriber {
    fn name(&self) -> &'static str {
        "proton-core-event-subscriber"
    }

    #[tracing::instrument(skip(self, events))]
    async fn on_events(&self, events: &mut [CoreEvent]) -> Result<(), SubscriberError> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("User context is no longer alive");
            return Ok(());
        };
        debug!("Handling {} events", events.len());

        let user_id = ctx.user_id().clone();
        let stash = ctx.stash().clone();

        let mut conn = stash.connection();
        conn.tx::<_, _, StashError>(async |tx| {
            for event in events.iter_mut() {
                handle_event(&ctx, event, tx, &user_id).await?;
            }
            Ok(())
        })
        .await
        .map_err(|e: StashError| SubscriberError::Other(anyhow!("Failed apply changes: {e}")))
    }

    async fn on_refresh(&self, event: &CoreEvent) -> Result<(), SubscriberError> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("User context is no longer alive");
            return Ok(());
        };

        ctx.on_refresh_impl(event.refresh).await
    }

    fn is_alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

async fn handle_event(
    ctx: &Arc<UserContext>,
    event: &mut CoreEvent,
    tx: &Bond<'_>,
    user_id: &UserId,
) -> Result<(), StashError> {
    if let Some(user) = event.user.as_mut() {
        debug!("Handling user event");

        // Update CoreAccount table:
        ctx.context
            .account_stash()
            .connection()
            .tx::<_, _, StashError>(async |account_tx| {
                if let Some(account) = CoreAccount::load(user.id(), account_tx).await? {
                    account
                        .with_display_name(user.display_name.clone().unwrap_or_default())
                        .with_name_or_addr(user.name.clone().unwrap_or_else(|| user.email.clone()))
                        .with_primary_addr(user.email.clone())
                        .with_username(user.name.clone().unwrap_or_default())
                        .save(account_tx)
                        .await
                } else {
                    Ok(())
                }
            })
            .await?;

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
        handle_address_event(tx, addresses).await?;
    }

    if let Some(labels) = event.labels.as_mut() {
        debug!("Handling label event");
        handle_label_events(tx, labels).await?;
    }

    if let Some(contacts) = event.contacts.as_mut() {
        debug!("Handling contact events");
        handle_contact_event(tx, contacts).await?;
    }
    if let Some(contact_emails) = event.contact_emails.as_mut() {
        debug!("Handling contact email events");
        handle_contact_email_event(tx, contact_emails).await?;
    }
    Ok(())
}

impl UserContext {
    pub async fn on_refresh_impl(&self, refresh: Refresh) -> Result<(), SubscriberError> {
        info!("Handling refresh event: {refresh:?}");

        match refresh {
            Refresh::None => {
                warn!("Nothing to refresh, this may idicate bug in SDK event loop implementation");
            }
            Refresh::Contacts => {
                try_refresh!(refresh_contacts, self);
            }
            Refresh::Mail => {
                // Mail refresh is handled by the mail context
            }
            Refresh::All => {
                try_refresh!(refresh_core, self);
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
    /// # Error
    ///
    /// Returns error if the event loop failed to register the subscriber.
    ///
    pub(crate) async fn register_subscribers(self: &Arc<Self>) -> Result<(), EventLoopError> {
        let event_loop_service = self.event_loop_service();

        event_loop_service
            .event_poll()
            .register(Box::new(self.event_subscriber()))
            .await?;

        Ok(())
    }

    /// Perform one iteration of the event loop, which consists of retrieving the latest events,
    /// publishing it on all the registered subscribers and storing the event id for the next
    /// iteration.
    ///
    /// The execution of the polling is aborted on the first error.
    ///
    /// # Error
    ///
    /// Returns error if the event loop failed to poll.
    ///
    pub async fn poll_event_loop_impl(&self) -> Result<(), EventLoopError> {
        let event_loop_service = self.event_loop_service();

        event_loop_service.event_poll().poll().await
    }

    #[must_use]
    pub fn event_subscriber(self: &Arc<Self>) -> impl Subscriber<CoreEvent> + 'static {
        CoreEventSubscriber::from(Arc::downgrade(self))
    }
}

#[tracing::instrument(skip_all)]
async fn refresh_core(ctx: &UserContext) -> Result<(), SubscriberError> {
    let api = ctx.session().clone();
    let contacts = ctx.spawn(async move { Contact::sync(&api).await });
    let api = ctx.session().clone();
    let all_remote_addresses = ctx.spawn(async move { Address::sync(&api).await });
    let api = ctx.session().clone();
    let user_and_settings = ctx.spawn(async move { User::sync_user_and_settings(&api).await });
    let api = ctx.session().clone();
    let all_remote_labels = ctx.spawn(async move { Label::fetch_contact_labels(&api).await });

    let mut tether = ctx.stash().connection();
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
        .tx::<_, _, SubscriberError>(async |tx| {
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

            Label::store_labels(tx, all_remote_labels)
                .await
                .map_err(|e| {
                    let e = anyhow!("Failed to sync labels: {e}");
                    error!("{e:?}");
                    SubscriberError::Other(e)
                })?;

            for local_label_to_remove in all_local_labels.into_values() {
                debug!(
                    "Removing label with remote_id {:?}",
                    local_label_to_remove.remote_id
                );
                local_label_to_remove.delete(tx).await?;
            }
            user_and_settings.store(tx).await?;
            contacts.store(tx).await?;

            Ok(())
        })
        .await
        .inspect_err(|e| {
            error!("Failed to update database entries while refreshing core: {e}");
        })?;

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn refresh_contacts(ctx: &UserContext) -> Result<(), SubscriberError> {
    let api = ctx.session().clone();
    let contacts = ctx.spawn(async move { Contact::sync(&api).await });
    let api = ctx.session().clone();
    let all_remote_labels = ctx.spawn(async move { Label::fetch_contact_labels(&api).await });
    let mut tether = ctx.stash().connection();
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
        .tx::<_, _, SubscriberError>(async |tx| {
            Label::store_labels(tx, all_remote_labels)
                .await
                .map_err(|e| {
                    let e = anyhow!("Failed to sync labels: {e}");
                    error!("{e:?}");
                    SubscriberError::Other(e)
                })?;

            for local_label_to_remove in all_local_labels.into_values() {
                debug!(
                    "Removing label with remote_id {:?}",
                    local_label_to_remove.remote_id
                );
                local_label_to_remove.delete(tx).await?;
            }
            contacts.store(tx).await?;

            Ok(())
        })
        .await
        .inspect_err(|e| {
            error!("Failed to update database entries while refreshing core: {e}");
        })?;

    Ok(())
}

async fn handle_address_event(
    tx: &Bond<'_>,
    address_events: &mut [AddressEvent],
) -> Result<(), StashError> {
    for event in address_events {
        event.action.log_entry(&event.remote_id);
        match event.action {
            Action::Delete => {
                warn!("[ET-1461] Delete action not implemented for address event");
            }

            Action::Create | Action::Update => {
                if let Some(ref mut address) = event.address {
                    address.save(tx).await?;
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
) -> Result<(), StashError> {
    for event in contact_events {
        event.action.log_entry(&event.remote_id);
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
                }
            }
            Action::UpdateFlags => (),
        }
    }
    Ok(())
}

async fn handle_contact_email_event(
    tx: &Bond<'_>,
    contact_email_events: &mut [ContactEmailEvent],
) -> Result<(), StashError> {
    for event in contact_email_events {
        event.action.log_entry(&event.remote_id);
        match event.action {
            Action::Delete => tx
                .execute(
                    "DELETE FROM contact_emails WHERE remote_id = ?",
                    params![event.remote_id.clone()],
                )
                .await
                .map(|_| ())
                .map_err(|e| {
                    error!("Failed to delete contact mail: {e:?}");
                    e
                })?,
            Action::Create | Action::Update => {
                if let Some(ref mut contact_email) = event.contact_email {
                    contact_email.save(tx).await.map_err(|e| {
                        error!("Failed to create or update contact mail: {e:?}");
                        e
                    })?;
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
) -> Result<(), StashError> {
    for label_event in label_events {
        label_event.action.log_entry(&label_event.remote_id);
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
                } else {
                    warn!("Received label create without label");
                }
            }
            Action::Update | Action::UpdateFlags => {
                if let Some(mut label) = label_event.label.clone() {
                    label.save(tx).await?;
                } else {
                    warn!("Received label update without label");
                }
            }
        }
    }
    Ok(())
}
