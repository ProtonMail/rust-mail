//! This module group all higher level validation functions

use crate::errors::VcardValidationResult;
use crate::properties::PropertyKind;

/// Extract the property name from a name who can contain a group name.
pub(super) fn get_property_kind(mut name: &str) -> VcardValidationResult<PropertyKind> {
    // Get the group name after the last dot
    if let Some(dot_index) = name.rfind('.') {
        name = &name[(dot_index + 1)..];
    }

    PropertyKind::try_from(name)
}
