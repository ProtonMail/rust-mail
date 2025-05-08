use std::fmt::Debug;

use ical::generator::Property as IcalProperty;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::preference::Preference;
use crate::parameters::value::ValueType;
use crate::properties::{VcardProperty, validate_parameters};
use crate::validation::get_property_kind;
use crate::values::text::Text;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To include extended XML-encoded vCard data in a plain vCard.
#[derive(Clone, Debug)]
pub struct Xml {
    /// Value
    pub value: Text,
    /// type of the value (here nothing or "text")
    pub value_type: Option<ValueType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl TryFrom<&IcalProperty> for Xml {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Xml));
        };
        let mut result = Self {
            value: value.into(),
            value_type: None,
            alternative_id: None,
            group: group_from_name(&property.name),
        };
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Xml))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Xml))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Xml,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Xml {
    fn get_preference(&self) -> Option<Preference> {
        None
    }
}

/// Validate that the given `property` respect the format for a `XML` property
///
/// # Errors
///   * if a parameter is invalid
///   * if property have no value
pub fn validate_xml(property: &IcalProperty) -> VcardValidationResult<()> {
    // XML-param = "VALUE=text" / altid-param
    // XML-value = text
    if property.value.is_some() {
        validate_parameters(
            property,
            ValueType::Text,
            &hash_set!(ParameterType::Value, ParameterType::AltId),
        )?;
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
