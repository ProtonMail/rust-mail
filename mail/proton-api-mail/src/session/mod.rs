use crate::domain::{MailEvent, MailSettings};
use crate::requests::GetMailSettingsRequest;
use proton_api_core::domain::EventId;
use proton_api_core::{http, Session};

mod conversations;
mod image_proxy;
mod labels;
mod messages;

/// Authenticated Session from which one can access mail related functionality
#[derive(Clone)]
pub struct MailSession {
    session: Session,
}

impl MailSession {
    #[must_use]
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    pub async fn event(&self, id: &EventId) -> Result<MailEvent, http::RequestError> {
        self.session.get_event::<MailEvent>(id).await
    }

    pub async fn mail_settings(&self) -> Result<MailSettings, http::RequestError> {
        self.session
            .execute_request(GetMailSettingsRequest {})
            .await
            .map(|r| r.mail_settings)
    }
}

impl From<Session> for MailSession {
    fn from(value: Session) -> Self {
        MailSession::new(value)
    }
}
