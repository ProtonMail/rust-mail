#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
// is exported in the root of the crate.
use crate::db::{CoreSqliteConnection, DBResult};
use proton_api_core::domain::{Event, ProductUsedSpace, User, UserId, UserSettings};
use proton_api_core::exports::anyhow;
use proton_api_core::exports::anyhow::anyhow;
use proton_api_core::exports::tracing::error;
use proton_event_loop::SubscriberError;

pub trait CoreEvent: Event {
    fn get_core_event_user(&self) -> Option<&User>;

    fn get_core_event_user_settings(&self) -> Option<&UserSettings>;

    fn get_core_event_used_space(&self) -> Option<i64>;

    fn get_core_event_used_product_space(&self) -> Option<&ProductUsedSpace>;
}

/// Since the core database can be embedded into another database, the integrator needs to provide
/// the subscriber with a way to access this database in order to make the required changes.
pub trait CoreEventSubscriberConnectionProvider: Send + Sync {
    /// Get the current user id and database connection.
    ///
    /// # Errors
    /// Return error if the connection or the user id can not be obtained.
    fn get_user_id_and_db_connection(&self) -> anyhow::Result<(UserId, CoreSqliteConnection)>;
}
pub struct CoreEventSubscriber<T: CoreEventSubscriberConnectionProvider>(T);

impl<T: CoreEventSubscriberConnectionProvider> CoreEventSubscriber<T> {
    pub fn new(provider: T) -> Self {
        Self(provider)
    }
}

#[async_trait]
impl<T: CoreEventSubscriberConnectionProvider, E: CoreEvent> proton_event_loop::Subscriber<E>
    for CoreEventSubscriber<T>
{
    fn name(&self) -> &str {
        "proton-core-subscriber"
    }

    async fn on_events(&self, events: &[E]) -> Result<(), SubscriberError> {
        let (user_id, mut conn) = self.0.get_user_id_and_db_connection().map_err(|e| {
            error!("Failed to get DB connection :{e}");
            SubscriberError::Other(anyhow!("Failed to get db connection: {e}"))
        })?;
        conn.tx(|tx| -> DBResult<()> {
            for event in events {
                if let Some(user) = event.get_core_event_user() {
                    tx.create_or_update_user(user).map_err(|e| {
                        error!("Failed to update user: {e}");
                        e
                    })?;
                }
                if let Some(settings) = event.get_core_event_user_settings() {
                    tx.create_or_update_user_settings(&user_id, settings)
                        .map_err(|e| {
                            error!("Failed to update user settings:{e}");
                            e
                        })?;
                }
                if let Some(used_space) = event.get_core_event_used_space() {
                    tx.update_user_used_space(&user_id, used_space)
                        .map_err(|e| {
                            error!("Failed to update used space:{e}");
                            e
                        })?;
                }
                if let Some(used_product_space) = event.get_core_event_used_product_space() {
                    tx.update_user_product_used_space(&user_id, used_product_space)
                        .map_err(|e| {
                            error!("Failed to update used product space: {e}");
                            e
                        })?;
                }
            }
            Ok(())
        })
        .map_err(|e| SubscriberError::Other(anyhow!("Failed apply changes: {e}")))
    }
}
