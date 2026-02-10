use std::collections::HashSet;
use std::fmt::Debug;

use ical::generator::Property as IcalProperty;

use crate::parameters::any::Any;
use crate::parameters::preference::Preference;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::values::uri::MaybeUri;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify a value that represents a globally unique identifier corresponding to the entity
/// associated with the vCard.
#[derive(Clone, Default, Debug)]
pub struct VcardUid {
    /// Value (ex: urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6)
    pub value: MaybeUri,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl TryFrom<&IcalProperty> for VcardUid {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::UId));
        };
        let mut value_type = None;
        let mut any = HashSet::new();
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::UId))?,
                        );
                    }
                    ParameterType::Any => {
                        any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::UId))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::UId,
                            parameter_type,
                        ));
                    }
                }
            }
        }

        Ok(Self {
            value: value.into(),
            value_type,
            any,
            group: group_from_name(&property.name),
        })
    }
}

impl VcardProperty for VcardUid {
    fn get_preference(&self) -> Option<Preference> {
        None
    }
}
