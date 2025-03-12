use std::collections::HashSet;
use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::any::Any;
use crate::parameters::preference::Preference;
use crate::parameters::value::ValueType;
use crate::properties::{VcardProperty, any_debug, optional_debug, validate_parameters};
use crate::validation::get_property_kind;
use crate::values::text::{Text, is_text_value};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the identifier for the product that created the vCard object.
#[derive(Clone)]
pub struct ProductId {
    /// Value
    pub value: Text,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl ProductId {
    /// Create a new PRODID property without any parameter or group (no check are done)
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self {
            value: Text::new_unchecked(value),
            value_type: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new PRODID property without any parameter or group
    ///
    /// # Errors
    ///   * if given value is not a valid text value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: Text::new_validated(value)
                .map_err(VCardError::from_value_error(PropertyKind::ProdId))?,
            value_type: None,
            any: HashSet::new(),
            group: None,
        })
    }
}

impl Debug for ProductId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProductId {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}",)
    }
}

impl TryFrom<&IcalProperty> for ProductId {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::ProdId));
        };
        let mut result = Self::new_validated(value.as_str())?;
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::ProdId))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::ProdId))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::ProdId,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for ProductId {
    fn get_preference(&self) -> Option<Preference> {
        None
    }
}

/// Validate that the given `property` respect the format for a `PRODID` property
///
/// # Errors
///   * if property value is not a valid text
///   * if any of the parameters is not valid
pub fn validate_prodid(property: &IcalProperty) -> VcardValidationResult<()> {
    // PRODID-param = "VALUE=text" / any-param
    // PRODID-value = text
    if let Some(value) = &property.value {
        if is_text_value(value) {
            validate_parameters(
                property,
                ValueType::Text,
                &hash_set!(ParameterType::Value, ParameterType::Any,),
            )?;
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
