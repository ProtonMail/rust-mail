use std::collections::HashSet;
use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use url::Url;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::any::{Any, is_any_param};
use crate::parameters::preference::Preference;
use crate::properties::{VcardProperty, any_debug, optional_debug};
use crate::validation::get_property_kind;
use crate::values::uri::Uri;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To give a global meaning to a local PID source identifier.
#[derive(Clone)]
pub struct ClientPidMap {
    /// index corresponding to second number in PIDs parameters
    pub index: u32,
    /// Unique identifier (ex: urn:uuid:3df403f4-5924-4bb7-b077-3c711d9eb34b)
    pub uri: Uri,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl ClientPidMap {
    /// Create a new CLIENTPIDMAP property, without any parameter or group
    #[must_use]
    pub fn new(index: u32, url: Url) -> Self {
        Self {
            index,
            uri: Uri::new(url),
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new CLIENTPIDMAP property
    ///
    /// # Errors
    ///   * if given value does not have the right format (ex: `1;urn:uuid:3df403f4-5924-4bb7-b077-3c711d9eb34b`)
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        let (index, uri) = Self::values_from_str(value)?;
        Ok(Self {
            index,
            uri,
            any: HashSet::new(),
            group: None,
        })
    }

    fn values_from_str(value: &str) -> VCardResult<(u32, Uri)> {
        if let Some((index, uri)) = value.split_once(';') {
            let index = index.parse().map_err(|_| {
                VCardError::InvalidValue(PropertyKind::ClientPIDMap, value.to_owned())
            })?;
            let uri = Uri::try_from(uri)
                .map_err(VCardError::from_value_error(PropertyKind::ClientPIDMap))?;
            Ok((index, uri))
        } else {
            Err(VCardError::InvalidValue(
                PropertyKind::ClientPIDMap,
                value.to_owned(),
            ))
        }
    }
}

impl Debug for ClientPidMap {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClientPidMap {{{:?} {:?}", self.index, self.uri)?;
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}")
    }
}

impl TryFrom<&IcalProperty> for ClientPidMap {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::ClientPIDMap));
        };
        let (index, uri) = Self::values_from_str(value)?;
        let mut result = Self {
            index,
            uri,
            any: HashSet::new(),
            group: group_from_name(&property.name),
        };
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Categories),
                            )?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::ClientPIDMap,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for ClientPidMap {
    fn get_preference(&self) -> Option<Preference> {
        None
    }
}

/// Validate that the given `property` respect the format for a `CLIENTPIDMAP` property
///
/// # Errors
///   * if property value is not a number followed by a semicolon followed by an uri (ex: `1;urn:uuid:3df403f4-5924-4bb7-b077-3c711d9eb34b`)
///   * if any of the parameters is not valid
pub fn validate_clientpidmap(property: &IcalProperty) -> VcardValidationResult<()> {
    // CLIENTPIDMAP-param = any-param
    // CLIENTPIDMAP-value = 1*DIGIT ";" URI
    if let Some(value) = &property.value {
        let Some((digits, uri)) = value.split_once(';') else {
            return Err(VcardValidationError::InvalidPropertyValue(
                get_property_kind(&property.name)?,
            ));
        };
        if digits.parse::<u32>().is_ok_and(|x| x > 0) && Url::parse(uri).is_ok() {
            if let Some(params) = &property.params {
                for (name, values) in params {
                    if !is_any_param(name, values) {
                        return Err(VcardValidationError::InvalidPropertyParam(
                            get_property_kind(&property.name)?,
                            name.to_owned(),
                        ));
                    }
                }
            }
        } else {
            return Err(VcardValidationError::InvalidPropertyValue(
                get_property_kind(&property.name)?,
            ));
        }
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
