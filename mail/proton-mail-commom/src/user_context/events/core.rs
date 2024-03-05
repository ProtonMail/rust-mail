use crate::WeakMailUserContext;
use proton_api_mail::proton_api_core::domain::UserId;
use proton_api_mail::proton_api_core::exports::anyhow;
use proton_api_mail::proton_api_core::exports::anyhow::anyhow;
use proton_api_mail::proton_api_core::exports::tracing::error;
use proton_core_common::proton_core_db::CoreSqliteConnection;
use proton_core_common::CoreEventSubscriberConnectionProvider;

impl CoreEventSubscriberConnectionProvider for WeakMailUserContext {
    fn get_user_id_and_db_connection(&self) -> anyhow::Result<(UserId, CoreSqliteConnection)> {
        let ctx = self.upgrade().ok_or_else(|| {
            let e = anyhow!("MailUserContext no longer alive");
            error!("{e}");
            e
        })?;

        let conn = ctx.inner.user_context.new_db_connection()?;
        Ok((ctx.user_id().clone(), conn))
    }
}
