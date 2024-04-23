use crate::domain::{AddressDomainLogoDetails, LightOrDarkMode};
use proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::http::{JsonResponse, Method, RequestData, RequestDesc};

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetAddressDomainLogoRequest {
    pub details: AddressDomainLogoDetails,
}

impl GetAddressDomainLogoRequest {
    #[must_use]
    pub fn new(details: AddressDomainLogoDetails) -> Self {
        Self { details }
    }
}

impl RequestDesc for GetAddressDomainLogoRequest {
    type Response = JsonResponse<GetAddressDomainLogoResponse>;

    fn build(&self) -> RequestData {
        let mut data = RequestData::new(Method::Get, "core/v4/images/logo");

        if let Some(address) = &self.details.address {
            data = data.query("Address", address);
        }

        if let Some(domain) = &self.details.domain {
            data = data.query("Domain", domain);
        }

        if let Some(size) = &self.details.size {
            data = data.query("Size", size);
        }

        if let Some(mode) = &self.details.mode {
            data = match mode {
                LightOrDarkMode::Light => data.query("Mode", &"Light"),
                LightOrDarkMode::Dark => data.query("Mode", &"Dark"),
            };
        }

        if let Some(bimi_selector) = &self.details.bimi_selector {
            data = data.query("BimiSelector", bimi_selector);
        }

        if let Some(max_scale_up_factor) = &self.details.max_scale_up_factor {
            data = data.query("MaxScaleUpFactor", max_scale_up_factor);
        }

        if let Some(format) = &self.details.format {
            data = data.query("Format", format);
        }

        data
    }
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetAddressDomainLogoResponse {
    pub image: Vec<u8>,
}
