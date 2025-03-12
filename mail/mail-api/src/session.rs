#![allow(async_fn_in_trait)]

use crate::services::proton::response_data::MailEvent;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::ProtonCore;
use proton_api_core::services::proton::common::EventId;
use proton_api_core::services::proton::prelude::GetEventOptions;
use proton_api_core::session::{CoreSession, Session};

/// Authenticated Session from which one can access mail related functionality
pub trait MailSession: CoreSession {
    async fn event(&self, id: EventId) -> Result<MailEvent, ApiServiceError> {
        self.api().get_event(id, GetEventOptions::default()).await
    }
}

impl MailSession for Session {}
