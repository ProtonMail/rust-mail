use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use crate::{
    CoreContextError, UserContext,
    datatypes::{ContactsDependencyFetcher, Refresh},
    events::{Action, AddressEvent, ContactEmailEvent, ContactEvent, CoreEvent},
    models::{Address, Contact, Label, ModelExtension, User},
};
use anyhow::{Context, anyhow, bail};
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
    stash::{Bond, StashError, Tether},
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

            while let Err(e) = $fn_name($ctx).await.inspect_err(|e| {
                match &e {
                    SubscriberError::Api(e) => {
                        if e.is_network_failure() {
                            return;
                        }
                    }
                    _ => {}
                }

                $ctx.issue_reporter_service().report(
                    IssueLevel::Critical,
                    format!("Failed to apply refresh event in {}", stringify!($fn_name)),
                    issue_report_keys_from_error(e),
                );
            }) {
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
use crate::event_loop::account_subscriber::AccountEventSubscriber;
use crate::events::LabelEvent;
use crate::models::{ContactEmail, ModelIdExtension};
pub use macros::*;

use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};

use proton_action_queue::action::ActionGroup;
use proton_action_queue::rebase::RebaseChangeSet;

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
        let tether = ctx.stash().connection().await?;
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
            .await?
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
        let mut conn = stash.connection().await?;

        let mut rebase_change_set = RebaseChangeSet::default();

        calculate_missing_dependencies(events, &conn)
            .await
            .context("Failed to calculate missing dependencies")?
            .fetch_and_store(ctx.session(), &mut conn)
            .await
            .context("Failed to fetch or store dependencies")?;

        conn.tx::<_, _, StashError>(async |tx| {
            for event in events.iter_mut() {
                handle_event(event, tx, &user_id, &mut rebase_change_set).await?;
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
                IssueLevel::Critical,
                "Failed to apply core event".into(),
                issue_report_keys_from_error(e),
            );
        })
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
    event: &mut CoreEvent,
    tx: &Bond<'_>,
    user_id: &UserId,
    rebase_change_set: &mut RebaseChangeSet,
) -> Result<(), StashError> {
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

    if let Some(contacts) = event.contacts.as_mut() {
        debug!("Handling contact events");
        handle_contact_event(tx, contacts, rebase_change_set).await?;
    }
    if let Some(contact_emails) = event.contact_emails.as_mut() {
        debug!("Handling contact email events");
        handle_contact_email_event(tx, contact_emails, rebase_change_set).await?;
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

        let event_poll = event_loop_service.event_poll();
        event_poll
            .register(Box::new(self.event_subscriber()))
            .await?;
        event_poll
            .register(Box::new(self.account_event_subscriber()))
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

    #[must_use]
    pub fn account_event_subscriber(self: &Arc<Self>) -> impl Subscriber<CoreEvent> + 'static {
        AccountEventSubscriber::from(Arc::downgrade(self))
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

            Label::store_labels_async(tx, all_remote_labels)
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

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn refresh_contacts(ctx: &UserContext) -> Result<(), SubscriberError> {
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

    Ok(())
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

async fn handle_contact_email_event(
    tx: &Bond<'_>,
    contact_email_events: &mut [ContactEmailEvent],
    rebase_change_set: &mut RebaseChangeSet,
) -> Result<(), StashError> {
    for event in contact_email_events {
        event
            .action
            .log_entry(&event.remote_id, async |remote_id| {
                ContactEmail::remote_id_counterpart(remote_id.clone(), tx)
                    .await
                    .unwrap_or_default()
                    .map(|v| v.as_u64())
            })
            .await;
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
                    rebase_change_set.add(contact_email.id());
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

async fn calculate_missing_dependencies(
    events: &[CoreEvent],
    tether: &Tether,
) -> Result<ContactsDependencyFetcher, CoreContextError> {
    let mut fetcher = ContactsDependencyFetcher::new();
    for event in events {
        if let Some(contact_emails) = event.contact_emails.as_ref() {
            for contact_email in contact_emails {
                if let Some(contact_email) = contact_email.contact_email.as_ref() {
                    fetcher.check_contact_email(contact_email, tether).await?;
                }
            }
        }
    }

    Ok(fetcher)
}
