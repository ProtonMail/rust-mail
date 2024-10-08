use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::have_no_param;
use crate::PropertyKind;
use ical::generator::Property;

/// Validate that the given `property` respect the format for a `END` property
///
/// # Errors
///   * if property value is not "VCARD"
///   * if any parameter is present
pub fn validate_end(property: &Property) -> VcardValidationResult<()> {
    // END-param = 0" "  ; no parameter allowed
    // END-value = "VCARD"
    if property.value == Some("VCARD".to_owned()) {
        if have_no_param(property.params.as_deref()) {
            Ok(())
        } else {
            Err(VcardValidationError::InvalidPropertyParam(
                PropertyKind::End,
                "no parameter allowed".to_owned(),
            ))
        }
    } else {
        Err(VcardValidationError::InvalidPropertyValue(
            PropertyKind::End,
        ))
    }
}
