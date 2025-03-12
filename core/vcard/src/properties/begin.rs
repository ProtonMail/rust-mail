use crate::PropertyKind;
use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::have_no_param;
use ical::generator::Property;

/// Validate that the given `property` respect the format for a `BEGIN` property
///
/// # Errors
///   * if property value is not "VCARD"
///   * if any parameter is present
pub fn validate_begin(property: &Property) -> VcardValidationResult<()> {
    // BEGIN-param = 0" "  ; no parameter allowed
    // BEGIN-value = "VCARD"
    if property.value == Some("VCARD".to_owned()) {
        if have_no_param(property.params.as_deref()) {
            Ok(())
        } else {
            Err(VcardValidationError::InvalidPropertyParam(
                PropertyKind::Begin,
                "no parameter allowed".to_owned(),
            ))
        }
    } else {
        Err(VcardValidationError::InvalidPropertyValue(
            PropertyKind::Begin,
        ))
    }
}
