use proton_api_core::exports::serde;
use proton_api_core::exports::serde::{Deserialize, Serialize};
use proton_api_core::exports::thiserror;

#[derive(Debug, thiserror::Error)]
pub enum AddressDomainLogoError {
    #[error("AddressDomainLogoDetails must include either an address or a domain")]
    NeitherAddressNorDomain(),
    #[error("max_scale_up_factor must be 1, 2, 3, or 4")]
    InvalidMaxScaleUpFactor(),
}

#[derive(Debug, Deserialize, Serialize)]
pub enum LightOrDarkMode {
    Light,
    Dark,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct AddressDomainLogoDetails {
    pub address: Option<String>,
    pub domain: Option<String>,
    pub size: Option<u32>,
    pub mode: Option<LightOrDarkMode>,
    pub bimi_selector: Option<String>,
    pub max_scale_up_factor: Option<u8>,
    pub format: Option<String>,
}

impl AddressDomainLogoDetails {
    pub fn new(
        address: Option<String>,
        domain: Option<String>,
        size: Option<u32>,
        mode: Option<LightOrDarkMode>,
        bimi_selector: Option<String>,
        max_scale_up_factor: Option<u8>,
        format: Option<String>,
    ) -> Result<Self, AddressDomainLogoError> {
        if address.is_none() && domain.is_none() {
            return Err(AddressDomainLogoError::NeitherAddressNorDomain());
        }

        if let Some(msup) = max_scale_up_factor {
            if msup == 0 || msup > 4 {
                return Err(AddressDomainLogoError::InvalidMaxScaleUpFactor());
            }
        }

        Ok(Self {
            address,
            domain,
            size,
            mode,
            bimi_selector,
            max_scale_up_factor,
            format,
        })
    }
}
