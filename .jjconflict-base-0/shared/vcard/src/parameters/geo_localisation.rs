use std::fmt::Debug;

use crate::errors::{VCardParameterError, VCardParameterResult};
use url::Url;

use crate::ParameterType;
use crate::values::uri::Uri;

/// The GEO parameter can be used to indicate global positioning information that is specific to an
/// address.
#[derive(Clone, Debug)]
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
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Ok(Self {
            value: Uri::new_validated(value)
                .map_err(VCardParameterError::from_value_error(ParameterType::Geo))?,
        })
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
    values.len() == 1 && {
        let value: &str = &values[0];
        // URI               ; from Section 3 of [RFC3986]
        Url::parse(value).is_ok()
    }
}
