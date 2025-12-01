use std::collections::HashSet;

use ical::generator::Property as IcalProperty;

use crate::parameters::any::Any;
use crate::parameters::preference::Preference;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the components of the sex and gender identity of the object the vCard represents.
#[derive(Clone, Debug)]
pub struct Gender {
    /// value (ex: O)
    pub value: GenderValue,
    /// type of the value (here nothing or "text")
    pub value_type: Option<ValueType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Gender {
    /// Create a new GENDER property without any parameter or group
    #[must_use]
    pub fn new(value: GenderValue) -> Self {
        Self {
            value,
            value_type: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new GENDER property without any parameter or group
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self::new(GenderValue::try_from(value)?))
    }
}

impl TryFrom<&IcalProperty> for Gender {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let value = property.value.as_ref().map_or("", |v| v.as_str());
        let mut result = Self {
            value: GenderValue::try_from(value)?,
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
                                .map_err(VCardError::from_parameter_error(PropertyKind::Gender))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Gender))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Gender,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Gender {
    fn get_preference(&self) -> Option<Preference> {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenderValue {
    Male(String),
    Female(String),
    Other(String),
    NotApplicable(String),
    Unknown(String),
    None(String),
    Custom(String),
}

impl TryFrom<&str> for GenderValue {
    type Error = VCardError;

    fn try_from(value: &str) -> VCardResult<Self> {
        let (value, message) = if let Some((value, message)) = value.split_once(';') {
            (value, message.to_owned())
        } else {
            (value, String::new())
        };
        match value {
            "M" | "m" => Ok(Self::Male(message)),
            "F" | "f" => Ok(Self::Female(message)),
            "O" | "o" => Ok(Self::Other(message)),
            "N" | "n" => Ok(Self::NotApplicable(message)),
            "U" | "u" => Ok(Self::Unknown(message)),
            "" => Ok(Self::None(message)),
            rest => Ok(Self::Custom(rest.to_owned())),
        }
    }
}
