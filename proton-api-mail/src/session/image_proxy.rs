use super::MailSession;
use crate::{
    domain::AddressDomainLogoDetails,
    requests::{GetAddressDomainLogoRequest, GetAddressDomainLogoResponse},
};
use proton_api_core::http;

impl MailSession {
    /// Request the logo for an address or domain via the API's image proxy
    pub async fn get_address_domain_logo(
        &self,
        request_details: AddressDomainLogoDetails,
    ) -> Result<GetAddressDomainLogoResponse, http::RequestError> {
        self.session
            .execute_request(GetAddressDomainLogoRequest::new(request_details))
            .await
    }
}
