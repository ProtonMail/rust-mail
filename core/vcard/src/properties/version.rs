use ical::generator::Property;
use velcro::hash_set;

use crate::ParameterType;
use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::value::ValueType;
use crate::properties::validate_parameters;
use crate::validation::get_property_kind;

/// Validate that the given `property` respect the format for a `VERSION` property
///
/// # Errors
///   * if the value is not `4.0`
///   * if any of the parameter is invalid
pub fn validate_version(property: &Property) -> VcardValidationResult<()> {
    // VERSION-param = "VALUE=text" / any-param
    // VERSION-value = "4.0"
    if let Some(value) = &property.value {
        if value == "4.0" {
            validate_parameters(
                property,
                ValueType::Text,
                &hash_set!(ParameterType::Value, ParameterType::Any),
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
