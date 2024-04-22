use crate::MailUserContext;
use proton_core_common::datatypes::RemoteId;
use proton_core_common::CoreEventSubscriberConnectionProvider;
use stash::stash::Stash;

impl CoreEventSubscriberConnectionProvider for MailUserContext {
    fn get_user_id_and_db_connection(&self) -> anyhow::Result<(RemoteId, Stash)> {
        Ok((self.user_id().clone(), self.user_context.stash().clone()))
    }
}
