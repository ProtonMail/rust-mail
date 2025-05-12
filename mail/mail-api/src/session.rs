#![allow(async_fn_in_trait)]

use crate::services::proton::response_data::MailEvent;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::EventId;
use proton_core_api::services::proton::GetEventOptions;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::session::{CoreSession, Session};

/// Authenticated Session from which one can access mail related functionality
pub trait MailSession: CoreSession {
    async fn event(&self, id: EventId) -> Result<MailEvent, ApiServiceError> {
        self.api().get_event(id, GetEventOptions::default()).await
    }
}

impl MailSession for Session {}
