#![allow(clippy::module_name_repetitions)]

use crate::datatypes::ProductUsedSpace;
use crate::events::{Action, AddressEvent, ContactEmailEvent, ContactEvent};
use crate::models::{User, UserSettings};
use anyhow::anyhow;
use async_trait::async_trait;
use futures::TryFutureExt;
use proton_api_core::services::proton::UserId;
use proton_event_loop::Event;
use proton_event_loop::subscriber::{Subscriber, SubscriberError};
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, Stash, StashError};
use std::sync::Weak;
use tracing::{Level, debug, error, warn};

pub trait CoreEvent: Event {
    fn get_core_event_user(&self) -> Option<&User>;
    fn get_core_event_user_mut(&mut self) -> Option<&mut User>;

    fn get_core_event_user_settings(&self) -> Option<&UserSettings>;
    fn get_core_event_user_settings_mut(&mut self) -> Option<&mut UserSettings>;

    fn get_core_event_addresses(&self) -> Option<&[AddressEvent]>;
    fn get_core_event_addresses_mut(&mut self) -> Option<&mut [AddressEvent]>;

    fn get_core_event_used_space(&self) -> Option<i64>;

    fn get_core_event_used_product_space(&self) -> Option<&ProductUsedSpace>;

    fn get_core_event_contacts(&self) -> Option<&[ContactEvent]>;
    fn get_core_event_contacts_mut(&mut self) -> Option<&mut [ContactEvent]>;

    fn get_core_event_contact_emails(&self) -> Option<&[ContactEmailEvent]>;
    fn get_core_event_contact_emails_mut(&mut self) -> Option<&mut [ContactEmailEvent]>;
}

/// Since the core database can be embedded into another database, the integrator needs to provide
/// the subscriber with a way to access this database in order to make the required changes.
#[async_trait]
pub trait CoreEventSubscriberConnectionProvider: Send + Sync {
    /// Get the current user id and database connection.
    ///
    /// # Errors
    /// Return error if the connection or the user id can not be obtained.
    async fn get_user_id_and_db_connection(&self) -> anyhow::Result<(UserId, Stash)>;
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
    fn name(&self) -> &'static str {
        "proton-core-subscriber"
    }

    #[tracing::instrument(level = Level::DEBUG, skip(self, events))]
    async fn on_events(&self, events: &mut [E]) -> Result<(), SubscriberError> {
        let (user_id, stash) = self
            .0
            .upgrade()
            .unwrap()
            .get_user_id_and_db_connection()
            .await
            .map_err(|e| {
                error!("Failed to get DB connection :{e:?}");
                SubscriberError::Other(anyhow!("Failed to get db connection: {e}"))
            })?;
        {
            let mut conn = stash.connection();
            conn.tx::<_, _, StashError>(async |tx| {
                for event in events.iter_mut() {
                    if let Some(user) = event.get_core_event_user_mut() {
                        debug!("Handling user event");
                        user.save(tx).await.map_err(|e| {
                            error!("Failed to update user: {e:?}");
                            e
                        })?;
                    }
                    if let Some(settings) = event.get_core_event_user_settings_mut() {
                        debug!("Handling user setting event");
                        settings.remote_id = Some(user_id.clone());
                        settings.save(tx).await.map_err(|e| {
                            error!("Failed to update user settings:{e:?}");
                            e
                        })?;
                    }
                    if let Some(used_space) = event.get_core_event_used_space() {
                        debug!("Handling user space event");
                        let mut user = User::load(user_id.clone(), tx).await?.unwrap();
                        user.used_space = used_space;
                        user.save(tx).await.map_err(|e| {
                            error!("Failed to update used space:{e:?}");
                            e
                        })?;
                    }
                    if let Some(used_product_space) = event.get_core_event_used_product_space() {
                        debug!("Handling user product space event");
                        let mut user = User::load(user_id.clone(), tx).await?.unwrap();
                        user.product_used_space = used_product_space.clone();
                        user.save(tx).await.map_err(|e| {
                            error!("Failed to update used space:{e:?}");
                            e
                        })?;
                    }
                    if let Some(addresses) = event.get_core_event_addresses_mut() {
                        debug!("Handling address event");
                        handle_address_event(tx, addresses).await?;
                    }
                    if let Some(contacts) = event.get_core_event_contacts_mut() {
                        debug!("Handling contact events");
                        handle_contact_event(tx, contacts).await?;
                    }
                    if let Some(contact_emails) = event.get_core_event_contact_emails_mut() {
                        debug!("Handling contact email events");
                        handle_contact_email_event(tx, contact_emails).await?;
                    }
                }
                Ok(())
            })
            .await
        }
        .map_err(|e: StashError| SubscriberError::Other(anyhow!("Failed apply changes: {e}")))
    }
}

async fn handle_address_event(
    tx: &Bond<'_>,
    address_events: &mut [AddressEvent],
) -> Result<(), StashError> {
    for event in address_events {
        match event.action {
            Action::Delete => {
                warn!("[ET-1461] Delete action not implemented for address event");
            }

            Action::Create | Action::Update => {
                if let Some(ref mut address) = event.address {
                    address
                        .save(tx)
                        .inspect_err(|e| error!("Failed to create or update address: {e:?}"))
                        .await?;
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
