use crate::UserContext;
use proton_api_core::domain::{IsEvent, User, UserProductUsedSpace, UserSettings};
use proton_api_core::exports::anyhow::anyhow;
use proton_api_core::exports::tracing::error;
use proton_core_db::{CoreSqliteConnection, DBResult};
use proton_event_loop::proton_async::async_trait::async_trait;
use proton_event_loop::SubscriberError;

#[async_trait]
pub trait CoreEvent: IsEvent {
    fn get_core_event_user(&self) -> Option<&User>;

    fn get_core_event_user_settings(&self) -> Option<&UserSettings>;

    fn get_core_event_used_space(&self) -> Option<i64>;

    fn get_core_event_used_product_space(&self) -> Option<&UserProductUsedSpace>;
}

pub struct CoreEventSubscriber {
    user_context: UserContext,
}

#[async_trait]
impl<E: CoreEvent> proton_event_loop::Subscriber<E> for CoreEventSubscriber {
    fn name(&self) -> &str {
        "proton-core-subscriber"
    }

    async fn on_events(&mut self, events: &[E]) -> Result<(), SubscriberError> {
        let mut conn = self
            .user_context
            .new_db_connection_as::<CoreSqliteConnection>()
            .map_err(|e| {
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
                    tx.create_or_update_user_settings(self.user_context.user_id(), settings)
                        .map_err(|e| {
                            error!("Failed to update user settings:{e}");
                            e
                        })?;
                }
                if let Some(used_space) = event.get_core_event_used_space() {
                    tx.update_user_used_space(self.user_context.user_id(), used_space)
                        .map_err(|e| {
                            error!("Failed to update used space:{e}");
                            e
                        })?;
                }
                if let Some(used_product_space) = event.get_core_event_used_product_space() {
                    tx.update_user_product_used_space(
                        self.user_context.user_id(),
                        used_product_space,
                    )
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
