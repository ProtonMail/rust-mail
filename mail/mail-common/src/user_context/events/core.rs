use crate::MailUserContext;
use async_trait::async_trait;
use proton_core_api::services::proton::UserId;
use proton_core_common::CoreEventSubscriberConnectionProvider;
use stash::stash::Stash;

#[async_trait]
impl CoreEventSubscriberConnectionProvider for MailUserContext {
    async fn get_user_id_and_db_connection(&self) -> anyhow::Result<(UserId, Stash)> {
        Ok((self.user_id().clone(), self.user_context.stash().clone()))
    }
}
