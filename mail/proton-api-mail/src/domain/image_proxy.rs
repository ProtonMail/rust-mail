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

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum LightOrDarkMode {
    Light,
    Dark,
}

#[derive(Debug, Default, Deserialize, Serialize)]
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

#[derive(Default)]
pub struct AddressDomainLogoDetailsBuilder(AddressDomainLogoDetails);

impl AddressDomainLogoDetailsBuilder {
    #[must_use]
    pub fn new() -> AddressDomainLogoDetailsBuilder {
        AddressDomainLogoDetailsBuilder::default()
    }

    #[must_use]
    pub fn address(mut self, address: String) -> AddressDomainLogoDetailsBuilder {
        self.0.address = Some(address);
        self
    }

    #[must_use]
    pub fn domain(mut self, domain: String) -> AddressDomainLogoDetailsBuilder {
        self.0.domain = Some(domain);
        self
    }

    #[must_use]
    pub fn size(mut self, size: u32) -> AddressDomainLogoDetailsBuilder {
        self.0.size = Some(size);
        self
    }

    #[must_use]
    pub fn mode(mut self, mode: LightOrDarkMode) -> AddressDomainLogoDetailsBuilder {
        self.0.mode = Some(mode);
        self
    }

    #[must_use]
    pub fn bimi_selector(mut self, bimi_selector: String) -> AddressDomainLogoDetailsBuilder {
        self.0.bimi_selector = Some(bimi_selector);
        self
    }

    pub fn max_scale_up_factor(
        mut self,
        max_scale_up_factor: u8,
    ) -> Result<AddressDomainLogoDetailsBuilder, AddressDomainLogoError> {
        if max_scale_up_factor == 0 || max_scale_up_factor > 4 {
            return Err(AddressDomainLogoError::InvalidMaxScaleUpFactor());
        }

        self.0.max_scale_up_factor = Some(max_scale_up_factor);
        Ok(self)
    }

    #[must_use]
    pub fn format(mut self, format: String) -> AddressDomainLogoDetailsBuilder {
        self.0.format = Some(format);
        self
    }

    pub fn build(self) -> Result<AddressDomainLogoDetails, AddressDomainLogoError> {
        if self.0.address.is_none() && self.0.domain.is_none() {
            return Err(AddressDomainLogoError::NeitherAddressNorDomain());
        }

        Ok(self.0)
    }
}
