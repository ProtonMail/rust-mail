use proton_api_rs::domain::{Event, EventId};
use proton_api_rs::http::Client;
use proton_api_rs::Session;
use proton_async::async_trait::async_trait;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Provider: Send + Sync {
    async fn get_latest_event_id(&self) -> proton_api_rs::http::Result<EventId>;

    async fn get_event(&self, event_id: &EventId) -> proton_api_rs::http::Result<Event>;
}

pub struct ProtonProvider {
    session: Session,
    client: Client,
}

impl ProtonProvider {
    pub fn new(client: Client, session: Session) -> Self {
        Self { client, session }
    }
}

#[async_trait]
impl Provider for ProtonProvider {
    async fn get_latest_event_id(&self) -> proton_api_rs::http::Result<EventId> {
        self.session.get_latest_event(&self.client).await
    }

    async fn get_event(&self, event_id: &EventId) -> proton_api_rs::http::Result<Event> {
        self.session.get_event(&self.client, event_id).await
    }
}
