use proton_api_core::exports::serde;
use proton_api_core::exports::serde::{Deserialize, Serialize};
use proton_api_core::exports::thiserror;

#[derive(Debug, thiserror::Error)]
pub enum AddressDomainLogoError {
    #[error("AddressDomainLogoDetails must include either an address or a domain")]
    NeitherAddressNorDomain(),
    #[error("max_scale_up_factor must be 1, 2, 3, or 4.  Value provided was: {0}")]
    InvalidMaxScaleUpFactor(u8),
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde")]
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

/// A builder used to create the struct for the body of requests to the API for logos for email addresses or
/// domains.  Note, these requests must have either and address or a domain value set so building the
/// [`AddressDomainLogoDetails`] (via ``build``) will fail if you haven't either set an address via the
/// ``address`` function or a domain via the ``domain`` function.
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

    /// Add a value for the maximum allowed scale up factor.  
    ///
    /// # Errors
    /// Will return a [`AddressDomainLogoError::InvalidMaxScaleUpFactor`] if the provided ``max_scale_up_factor``
    /// is not a 1, 2, 3, or 4.
    pub fn max_scale_up_factor(
        mut self,
        max_scale_up_factor: u8,
    ) -> Result<AddressDomainLogoDetailsBuilder, AddressDomainLogoError> {
        if max_scale_up_factor == 0 || max_scale_up_factor > 4 {
            return Err(AddressDomainLogoError::InvalidMaxScaleUpFactor(
                max_scale_up_factor,
            ));
        }

        self.0.max_scale_up_factor = Some(max_scale_up_factor);
        Ok(self)
    }

    #[must_use]
    pub fn format(mut self, format: String) -> AddressDomainLogoDetailsBuilder {
        self.0.format = Some(format);
        self
    }

    /// Returns the constructed [`AddressDomainLogoDetails`]
    ///
    /// # Errors
    /// Will return an error if there is neither an ``address`` nor a ``domain`` set in the builder.
    pub fn build(self) -> Result<AddressDomainLogoDetails, AddressDomainLogoError> {
        if self.0.address.is_none() && self.0.domain.is_none() {
            return Err(AddressDomainLogoError::NeitherAddressNorDomain());
        }

        Ok(self.0)
    }
}
