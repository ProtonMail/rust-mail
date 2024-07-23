#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
// avoid namespace conflicts
use crate::Event;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId;
use proton_api_core::session::{CoreSession, Session};

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Provider<T: Event + From<<T as Event>::Response>>: Send + Sync {
    async fn get_latest_event_id(&self) -> Result<RemoteId, ApiServiceError>;

    async fn get_event(&self, event_id: &RemoteId) -> Result<T, ApiServiceError>;
}

pub struct ProtonProvider {
    session: Session,
}

impl ProtonProvider {
    #[must_use]
    pub fn new(session: Session) -> Self {
        Self { session }
    }
}

#[async_trait]
impl<T: Event + From<<T as Event>::Response>> Provider<T> for ProtonProvider {
    async fn get_latest_event_id(&self) -> Result<RemoteId, ApiServiceError> {
        Ok(self.session.api().get_events_latest().await?.event_id)
    }

    async fn get_event(&self, event_id: &RemoteId) -> Result<T, ApiServiceError> {
        Ok(self
            .session
            .api()
            .get_event::<T::Response>(event_id.clone(), false, false)
            .await?
            .into())
    }
}
