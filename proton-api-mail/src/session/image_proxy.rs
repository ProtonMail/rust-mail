use super::MailSession;
use crate::{
    domain::AddressDomainLogoDetails,
    requests::{GetAddressDomainLogoRequest, GetAddressDomainLogoResponse},
};
use proton_api_core::http;

impl MailSession {
    pub async fn get_address_domain_logo(
        &self,
        request_details: AddressDomainLogoDetails,
    ) -> Result<GetAddressDomainLogoResponse, http::RequestError> {
        self.session
            .execute_request(GetAddressDomainLogoRequest::new(request_details))
            .await
    }
}
