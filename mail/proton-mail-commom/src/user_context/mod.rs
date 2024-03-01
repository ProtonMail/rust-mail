mod conversations;
mod initialization;
mod labels;
mod queries;

pub use initialization::*;

use crate::MailContextResult;
use proton_api_mail::proton_api_core::domain::UserId;
use proton_api_mail::proton_api_core::exports::proton_sqlite3::InProcessTrackerService;
use proton_api_mail::MailSession;
use proton_core_common::proton_core_db::DBResult;
use proton_core_common::UserContext;
use proton_mail_db::MailSqliteConnection;

#[derive(Debug, Clone)]
pub struct MailUserContext(UserContext);

impl MailUserContext {
    pub(crate) fn new(user_context: UserContext) -> Self {
        Self(user_context)
    }

    pub(crate) fn mail_session(&self) -> MailSession {
        self.0.session_as::<MailSession>()
    }

    pub(crate) fn new_db_connection(&self) -> DBResult<MailSqliteConnection> {
        self.0.new_db_connection_as::<MailSqliteConnection>()
    }

    pub(crate) fn tracker_service(&self) -> &InProcessTrackerService {
        self.0.tracker_service()
    }

    pub fn user_id(&self) -> &UserId {
        self.0.user_id()
    }

    pub async fn logout(&self) -> MailContextResult<()> {
        self.0.session().logout().await?;
        Ok(())
    }
}
