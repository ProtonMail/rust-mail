use std::collections::HashSet;

use ical::generator::Property as IcalProperty;

use crate::parameters::any::Any;
use crate::parameters::value::ValueType;

use crate::values::iana_token::{IanaToken, is_iana_token_value};
use crate::values::x_name::{XName, is_x_name_value};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the kind of object the vCard represents.
#[derive(Clone, Debug)]
pub struct Kind {
    /// Value
    pub value: KindValue,
    /// type of the value (here nothing or "text")
    pub value_type: Option<ValueType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Kind {
    /// Create a new KIND property without any parameter or group
    #[must_use]
    pub fn new(value: KindValue) -> Self {
        Self {
            value,
            value_type: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new KIND property without any parameter or group
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self::new(KindValue::try_from(value)?))
    }
}

impl TryFrom<&IcalProperty> for Kind {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Kind));
        };
        let mut result = Self {
            value: KindValue::try_from(value.as_str())?,
            value_type: None,
            any: HashSet::new(),
            group: group_from_name(&property.name),
        };
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Kind))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Kind))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Kind,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

/// Possible values for Kind property
#[derive(Clone, Debug, PartialEq)]
pub enum KindValue {
    /// Individual
    Individual,
    /// Group
    Group,
    /// Organization
    Organization,
    /// Location
    Location,
    /// Iana token
    IanaToken(IanaToken),
    /// X name
    XName(XName),
}

impl TryFrom<&str> for KindValue {
    type Error = VCardError;

    fn try_from(value: &str) -> VCardResult<Self> {
        match value.to_ascii_lowercase().as_str() {
            "individual" => Ok(Self::Individual),
            "group" => Ok(Self::Group),
            "org" => Ok(Self::Organization),
            "location" => Ok(Self::Location),
            _ => {
                if is_x_name_value(value) {
                    Ok(Self::XName(XName::try_from(value).map_err(
                        VCardError::from_value_error(PropertyKind::Kind),
                    )?))
                } else if is_iana_token_value(value) {
                    Ok(Self::IanaToken(IanaToken::try_from(value).map_err(
                        VCardError::from_value_error(PropertyKind::Kind),
                    )?))
                } else {
                    Err(VCardError::InvalidValue(
                        PropertyKind::Kind,
                        value.to_owned(),
                    ))
                }
            }
        }
    }
}
