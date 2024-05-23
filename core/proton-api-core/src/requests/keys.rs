use crate::http::{JsonResponse, Method, RequestData, RequestDesc};
use serde::{self, Serialize};

use proton_crypto_account::domain::APIPublicAddressKeys;

#[derive(Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetAllActiveKeysRequest {
    email: String,
    internal_keys_only: Option<bool>,
}

impl GetAllActiveKeysRequest {
    #[must_use]
    pub fn new(email: String, internal_keys_only: Option<bool>) -> GetAllActiveKeysRequest {
        GetAllActiveKeysRequest {
            email,
            internal_keys_only,
        }
    }
}

impl RequestDesc for GetAllActiveKeysRequest {
    type Response = JsonResponse<APIPublicAddressKeys>;

    fn build(&self) -> RequestData {
        let mut request_data =
            RequestData::new(Method::Get, "core/v4/keys/all").query("Email", &self.email);

        if let Some(val) = self.internal_keys_only {
            let internal_only = if val { "1" } else { "0" };
            request_data = request_data.query("InternalOnly", &internal_only);
        }

        request_data
    }
}
