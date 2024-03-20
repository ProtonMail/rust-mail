use super::MailSession;
use crate::domain::Address;
use crate::requests::GetAddressesRequest;
use proton_api_core::http;

impl MailSession {
    pub async fn addresses(&self) -> Result<Vec<Address>, http::HttpRequestError> {
        self.session
            .execute_request(GetAddressesRequest {})
            .await
            .map(|v| v.addresses)
    }
}
