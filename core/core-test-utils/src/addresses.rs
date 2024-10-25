use crate::test_context::TestContext;
use lazy_static::lazy_static;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::Address as ApiAddress;
use proton_api_core::services::proton::responses::{GetAddressResponse, GetAddressesResponse};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

lazy_static! {
    pub static ref MY_ADDRESS_ID: ApiRemoteId = ApiRemoteId::from("MyRemoteId");
}

impl TestContext {
    pub async fn mock_get_all_addresses(&self, addresses: Vec<ApiAddress>) {
        let response = GetAddressesResponse { addresses };

        Mock::given(method("GET"))
            .and(path("/api/core/v4/addresses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(self.mock_server())
            .await;
    }

    pub async fn mock_get_address(&self, address: ApiAddress) {
        let response = GetAddressResponse {
            address: address.clone(),
        };

        Mock::given(method("GET"))
            .and(path(format!("/api/core/v4/addresses/{}", address.id)))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(self.mock_server())
            .await;
    }
}
