mod events;
mod initialization;
mod settings;

use crate::mail::{map_task_join_error, MailContextError};
use proton_mail_common as pmc;
use std::sync::Arc;

#[derive(uniffi::Object)]
pub struct MailUserContext {
    ctx: pmc::MailUserContext,
}

impl MailUserContext {
    pub(crate) fn new(ctx: pmc::MailUserContext) -> Arc<Self> {
        Arc::new(Self { ctx })
    }
    pub(crate) fn ctx(&self) -> &pmc::MailUserContext {
        &self.ctx
    }
}

#[uniffi::export]
impl MailUserContext {
    /// Log out a session.
    pub async fn logout(&self) -> Result<(), MailContextError> {
        let ctx = self.ctx().clone();
        let handle = self
            .ctx
            .mail_context()
            .async_runtime()
            .spawn(async move { ctx.logout().await });
        handle.await.map_err(map_task_join_error)??;
        Ok(())
    }
}
