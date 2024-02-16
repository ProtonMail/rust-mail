use proton_api_core::domain::{EventId, IsEvent};
use proton_api_core::Session;
use proton_async::async_trait::async_trait;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Provider<T: IsEvent>: Send + Sync {
    async fn get_latest_event_id(&self) -> proton_api_core::http::Result<EventId>;

    async fn get_event(&self, event_id: &EventId) -> proton_api_core::http::Result<T>;
}

pub struct ProtonProvider {
    session: Session,
}

impl ProtonProvider {
    pub fn new(session: Session) -> Self {
        Self { session }
    }
}

#[async_trait]
impl<T: IsEvent> Provider<T> for ProtonProvider {
    async fn get_latest_event_id(&self) -> proton_api_core::http::Result<EventId> {
        self.session.get_latest_event().await
    }

    async fn get_event(&self, event_id: &EventId) -> proton_api_core::http::Result<T> {
        self.session.get_event::<T>(event_id).await
    }
}
