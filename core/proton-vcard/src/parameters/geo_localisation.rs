use std::fmt::{Debug, Formatter};

use crate::errors::{VCardParameterError, VCardParameterResult};
use url::Url;

use crate::values::uri::{is_uri_value, Uri};
use crate::ParameterType;

/// The GEO parameter can be used to indicate global positioning information that is specific to an
/// address.
#[derive(Clone)]
pub struct GeoLocalisation {
    /// Value
    pub value: Uri,
}

impl GeoLocalisation {
    /// Create a new geo parameter from an Url
    #[must_use]
    pub fn new_unchecked(value: Url) -> Self {
        Self {
            value: Uri::new(value),
        }
    }

    /// Try to create a new geo parameter form a str
    ///
    /// # Errors
    ///   * value is not a valid URL
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Ok(Self {
            value: Uri::new_validated(value)
                .map_err(VCardParameterError::from_value_error(ParameterType::Geo))?,
        })
    }
}

impl Debug for GeoLocalisation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.value)
    }
}

impl TryFrom<&[String]> for GeoLocalisation {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if values.len() != 1 {
            return Err(VCardParameterError::ExpectedExactlyOneValue(
                ParameterType::Geo,
                values.to_vec(),
            ));
        }
        Ok(Self {
            value: Uri::try_from(values[0].as_str())
                .map_err(VCardParameterError::from_value_error(ParameterType::Geo))?,
        })
    }
}

/// Validate that the given `values` respect the format for a `GEO` parameter
#[must_use]
pub fn is_geo_param(values: &[String]) -> bool {
    // geo-parameter = "GEO=" DQUOTE URI DQUOTE
    values.len() == 1 && is_uri_value(&values[0])
}
