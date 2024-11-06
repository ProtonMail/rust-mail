#![allow(clippy::module_name_repetitions)]

use crate::datatypes::{ProductUsedSpace, RemoteId};
use crate::events::{Action, ContactEmailEvent, ContactEvent};
use crate::models::{Address, User, UserSettings};
use anyhow::anyhow;
use async_trait::async_trait;
use proton_event_loop::subscriber::{Subscriber, SubscriberError};
use proton_event_loop::Event;
use stash::orm::Model;
use stash::params;
use stash::stash::{Interface, Stash, StashError, Tether};
use std::sync::Weak;
use tracing::{debug, error, Level};

pub trait CoreEvent: Event {
    fn get_core_event_user(&self) -> Option<&User>;
    fn get_core_event_user_mut(&mut self) -> Option<&mut User>;

    fn get_core_event_user_settings(&self) -> Option<&UserSettings>;
    fn get_core_event_user_settings_mut(&mut self) -> Option<&mut UserSettings>;

    fn get_core_event_addresses(&self) -> Option<&[Address]>;
    fn get_core_event_addresses_mut(&mut self) -> Option<&mut [Address]>;

    fn get_core_event_used_space(&self) -> Option<i64>;

    fn get_core_event_used_product_space(&self) -> Option<&ProductUsedSpace>;

    fn get_core_event_contacts(&self) -> Option<&[ContactEvent]>;
    fn get_core_event_contacts_mut(&mut self) -> Option<&mut [ContactEvent]>;

    fn get_core_event_contact_emails(&self) -> Option<&[ContactEmailEvent]>;
    fn get_core_event_contact_emails_mut(&mut self) -> Option<&mut [ContactEmailEvent]>;
}

/// Since the core database can be embedded into another database, the integrator needs to provide
/// the subscriber with a way to access this database in order to make the required changes.
pub trait CoreEventSubscriberConnectionProvider: Send + Sync {
    /// Get the current user id and database connection.
    ///
    /// # Errors
    /// Return error if the connection or the user id can not be obtained.
    fn get_user_id_and_db_connection(&self) -> anyhow::Result<(RemoteId, Stash)>;
}
pub struct CoreEventSubscriber<T: CoreEventSubscriberConnectionProvider>(Weak<T>);

impl<T: CoreEventSubscriberConnectionProvider> CoreEventSubscriber<T> {
    #[must_use]
    pub fn new(provider: Weak<T>) -> Self {
        Self(provider)
    }
}

#[async_trait]
impl<T: CoreEventSubscriberConnectionProvider, E: CoreEvent> Subscriber<E>
    for CoreEventSubscriber<T>
{
    fn name(&self) -> &str {
        "proton-core-subscriber"
    }

    #[tracing::instrument(level = Level::DEBUG, skip(self, events))]
    async fn on_events(&self, events: &mut [E]) -> Result<(), SubscriberError> {
        let (user_id, stash) = self
            .0
            .upgrade()
            .unwrap()
            .get_user_id_and_db_connection()
            .map_err(|e| {
                error!("Failed to get DB connection :{e}");
                SubscriberError::Other(anyhow!("Failed to get db connection: {e}"))
            })?;
        {
            let tx = stash.transaction().await.map_err(|e| {
                error!("Failed to start transaction: {e}");
                SubscriberError::Other(anyhow!("Failed to start transaction: {e}"))
            })?;

            for event in events.iter_mut() {
                if let Some(user) = event.get_core_event_user_mut() {
                    debug!("Handling user event");
                    user.set_stash(&stash);
                    user.save_using(&tx).await.map_err(|e| {
                        error!("Failed to update user: {e}");
                        e
                    })?;
                }
                if let Some(settings) = event.get_core_event_user_settings_mut() {
                    debug!("Handling user setting event");
                    settings.remote_id = Some(user_id.clone());
                    settings.set_stash(&stash);
                    settings.save_using(&tx).await.map_err(|e| {
                        error!("Failed to update user settings:{e}");
                        e
                    })?;
                }
                if let Some(used_space) = event.get_core_event_used_space() {
                    debug!("Handling user space event");
                    let mut user = User::load(user_id.clone(), &stash).await?.unwrap();
                    user.used_space = used_space;
                    user.set_stash(&stash);
                    user.save_using(&tx).await.map_err(|e| {
                        error!("Failed to update used space:{e}");
                        e
                    })?;
                }
                if let Some(used_product_space) = event.get_core_event_used_product_space() {
                    debug!("Handling user product space event");
                    let mut user = User::load(user_id.clone(), &stash).await?.unwrap();
                    user.product_used_space = used_product_space.clone();
                    user.set_stash(&stash);
                    user.save_using(&tx).await.map_err(|e| {
                        error!("Failed to update used space:{e}");
                        e
                    })?;
                }
                if let Some(addresses) = event.get_core_event_addresses_mut() {
                    debug!("Handling address event");
                    for address in addresses {
                        address.save_using(&tx).await.map_err(|e| {
                            error!("Failed to update user addresses: {e}");
                            e
                        })?;
                    }
                }
                if let Some(contacts) = event.get_core_event_contacts_mut() {
                    debug!("Handling contact events");
                    handle_contact_event(&tx, contacts).await?;
                }
                if let Some(contact_emails) = event.get_core_event_contact_emails_mut() {
                    debug!("Handling contact email events");
                    handle_contact_email_event(&tx, contact_emails).await?;
                }
            }

            tx.commit().await?;

            Ok(())
        }
        .map_err(|e: StashError| SubscriberError::Other(anyhow!("Failed apply changes: {e}")))
    }
}

async fn handle_contact_event(
    tx: &Tether,
    contact_events: &mut [ContactEvent],
) -> Result<(), StashError> {
    for event in contact_events {
        match event.action {
            Action::Delete => tx
                .execute(
                    "DELETE FROM contacts WHERE remote_id = ?",
                    params![event.remote_id.clone()],
                )
                .await
                .map(|_| ())
                .map_err(|e| {
                    error!("Failed to delete contact: {e}");
                    e
                })?,
            Action::Create | Action::Update => {
                if let Some(ref mut contact) = event.contact {
                    contact.save_using(tx).await.map_err(|e| {
                        error!("Failed to create or update contact: {e}");
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
    tx: &Tether,
    contact_email_events: &mut [ContactEmailEvent],
) -> Result<(), StashError> {
    for event in contact_email_events {
        match event.action {
            Action::Delete => tx
                .execute(
                    "DELETE FROM contact_emails WHERE remote_id = ?",
                    params![event.remote_id.clone()],
                )
                .await
                .map(|_| ())
                .map_err(|e| {
                    error!("Failed to delete contact mail: {e}");
                    e
                })?,
            Action::Create | Action::Update => {
                if let Some(ref mut contact_email) = event.contact_email {
                    contact_email.save_using(tx).await.map_err(|e| {
                        error!("Failed to create or update contact mail: {e}");
                        e
                    })?;
                }
            }
            Action::UpdateFlags => (),
        }
    }
    Ok(())
}
