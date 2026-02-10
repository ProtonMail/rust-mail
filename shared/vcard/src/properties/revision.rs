use std::collections::HashSet;

use ical::generator::Property as IcalProperty;

use crate::parameters::any::Any;
use crate::parameters::value::ValueType;
use crate::values::timestamp::Timestamp;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify revision information about the current vCard.
#[derive(Clone, Debug)]
pub struct Revision {
    /// Value (ex: 19951031T222710Z)
    pub value: Timestamp,
    /// type of the value (here nothing or "timestamp")
    pub value_type: Option<ValueType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this Revision belong to
    pub group: Option<String>,
}

impl Revision {
    /// Create a new REV property without any parameter or group
    #[must_use]
    pub fn new(value: Timestamp) -> Self {
        Self {
            value,
            value_type: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new REV property without any parameter or group
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self::new(Timestamp::try_from(value).map_err(
            VCardError::from_value_error(PropertyKind::Rev),
        )?))
    }
}

impl TryFrom<&IcalProperty> for Revision {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Rev));
        };
        let mut result = Self::new_validated(value.as_str())?;
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Rev))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Rev))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Rev,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}
