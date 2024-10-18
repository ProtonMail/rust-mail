use crate::common::TestContext;
use lazy_static::lazy_static;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::Address as ApiAddress;
use proton_api_core::services::proton::responses::{GetAddressResponse, GetAddressesResponse};
use proton_core_common::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType,
};
use proton_core_common::models::Address;
use stash::stash::Tether;
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

pub async fn create_address(core_tx: &Tether) -> Address {
    let mut address = test_address();
    address
        .save_using(core_tx)
        .await
        .expect("failed to create address");

    address
}

pub fn test_address() -> Address {
    Address {
        local_id: None,
        remote_id: Some(MY_ADDRESS_ID.clone().into()),
        email: "hello@world".to_owned(),
        send: Default::default(),
        receive: Default::default(),
        status: AddressStatus::Enabled,
        domain_id: None,
        address_type: AddressType::Original,
        display_order: 0,
        display_name: "HelloWorld".to_owned(),
        signature: "SIGNATURE".to_owned(),
        keys: AddressKeys::default(),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList {
            min_epoch_id: None,
            max_epoch_id: None,
            expected_min_epoch_id: None,
            data: None,
            obsolescence_token: None,
            signature: None,
            revision: 0,
        },
        row_id: None,
        stash: None,
    }
}
