#![allow(async_fn_in_trait)]

use crate::services::proton::response_data::MailEvent;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId;
use proton_api_core::session::{CoreSession, Session};

/// Authenticated Session from which one can access mail related functionality
pub trait MailSession: CoreSession {
    async fn event(&self, id: RemoteId) -> Result<MailEvent, ApiServiceError> {
        self.api().get_event::<MailEvent>(id, false, false).await
    }
}

impl MailSession for Session {}
