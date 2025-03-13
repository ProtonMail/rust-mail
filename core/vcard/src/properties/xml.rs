use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::preference::Preference;
use crate::parameters::value::ValueType;
use crate::properties::{optional_debug, validate_parameters, VcardProperty};
use crate::validation::get_property_kind;
use crate::values::text::{is_text_value, Text};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To include extended XML-encoded vCard data in a plain vCard.
#[derive(Clone)]
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

impl Xml {
    /// Create a new Xml property
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self {
            value: Text::new_unchecked(value),
            value_type: None,
            alternative_id: None,
            group: None,
        }
    }

    /// Try to create new Xml property
    ///
    /// # Errors
    ///   * if given value is not a valid text value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: Text::new_validated(value)
                .map_err(VCardError::from_value_error(PropertyKind::Xml))?,
            value_type: None,
            alternative_id: None,
            group: None,
        })
    }
}

impl Debug for Xml {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Xml {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        optional_debug!(self, f, group, group);
        write!(f, "}}")
    }
}

impl TryFrom<&IcalProperty> for Xml {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Xml));
        };
        let mut result = Self {
            value: Text::try_from(value.as_str())
                .map_err(VCardError::from_value_error(PropertyKind::Xml))?,
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
    if let Some(value) = &property.value {
        if is_text_value(value) {
            validate_parameters(
                property,
                ValueType::Text,
                &hash_set!(ParameterType::Value, ParameterType::AltId),
            )?;
        }
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
