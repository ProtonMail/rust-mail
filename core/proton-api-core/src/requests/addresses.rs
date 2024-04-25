use crate::domain::Address;
use crate::http::{JsonResponse, Method, RequestData, RequestDesc};
use serde::Serialize;
use serde::{self, Deserialize};

#[derive(Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetAddressesResponse {
    pub addresses: Vec<Address>,
}

pub struct GetAddressesRequest {}

impl RequestDesc for GetAddressesRequest {
    type Response = JsonResponse<GetAddressesResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, "core/v4/addresses")
    }
}
