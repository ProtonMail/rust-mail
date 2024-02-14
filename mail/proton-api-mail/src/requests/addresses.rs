use crate::domain::Address;
use proton_api_core::exports::serde::{self, Deserialize};
use proton_api_core::http::{JsonResponse, Method, RequestData, RequestDesc};

#[derive(Deserialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetAddressesResponse {
    pub addresses: Vec<Address>,
}

pub struct GetAddressesRequest {}

impl RequestDesc for GetAddressesRequest {
    type Output = GetAddressesResponse;
    type Response = JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, "core/v4/addresses")
    }
}
