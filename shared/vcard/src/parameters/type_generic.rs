use std::collections::HashSet;

use crate::errors::{VCardParameterError, VCardParameterResult};
use crate::parameters::type_related::RELATED_VALUES;
use crate::parameters::type_tel::TEL_VALUES;
use crate::values::iana_token::{IanaToken, is_iana_token_value};
use crate::values::x_name::{XName, is_x_name_value};
use crate::{ParameterType, PropertyKind};

/// The TYPE parameter has multiple, different uses.  In general, it is a way of specifying class
/// characteristics of the associated property.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum GenericType {
    Home,
    Work,
    IanaToken(IanaToken),
    XName(XName),
}

impl GenericType {
    /// Try to create a new generic TYPE parameter
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Self::try_from(value)
    }

    /// Try to create a new `HashSet` of TYPE parameters
    pub fn set_from_values(values: &[String]) -> VCardParameterResult<HashSet<Self>> {
        values
            .iter()
            .filter(|value| !value.trim().is_empty())
            .map(|v| Self::try_from(v.as_str()))
            .collect()
    }
}

impl TryFrom<&str> for GenericType {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        match value.to_ascii_lowercase().as_ref() {
            "home" => Ok(Self::Home),
            "work" => Ok(Self::Work),
            _ => {
                if is_x_name_value(value) {
                    Ok(Self::XName(XName::new_unchecked(value)))
                } else if is_iana_token_value(value) {
                    Ok(Self::IanaToken(IanaToken::new_unchecked(value)))
                } else {
                    Err(VCardParameterError::InvalidValue(
                        ParameterType::Type,
                        value.to_owned(),
                    ))
                }
            }
        }
    }
}

/// Validate that the given `values` respect the format for a `TYPE` parameter depending on the associated property kind.
/// `TEL` and `RELATED` have special rules, other kinds are equivalents between them.
#[must_use]
pub fn is_type_param(property: &PropertyKind, values: &[String]) -> bool {
    // type-param = "TYPE=" type-value *("," type-value)
    // type-value = "work" / "home" / type-param-tel / type-param-related / iana-token / x-name
    //          ; This is further defined in individual property sections.

    // Only if property is "TEL"
    // type-param-tel = "text" / "voice" / "fax" / "cell" / "video" / "pager" / "textphone" / iana-token / x-name
    //    ; type-param-tel MUST NOT be used with a property other than TEL.
    fn if_tel(property: &PropertyKind, value: &str) -> bool {
        !matches!(property, PropertyKind::Tel)
            || (TEL_VALUES.contains(&value.to_ascii_lowercase().as_str())
                || is_iana_token_value(value)
                || is_x_name_value(value))
    }

    // Only if property is "RELATED"
    // type-param-related = related-type-value *("," related-type-value)
    //    ; type-param-related MUST NOT be used with a property other than
    //    ; RELATED.
    //  related-type-value = "contact" / "acquaintance" / "friend" / "met"
    //                     / "co-worker" / "colleague" / "co-resident"
    //                     / "neighbor" / "child" / "parent"
    //                     / "sibling" / "spouse" / "kin" / "muse"
    //                     / "crush" / "date" / "sweetheart" / "me"
    //                     / "agent" / "emergency"
    fn if_related(property: &PropertyKind, value: &str) -> bool {
        !matches!(property, PropertyKind::Related)
            || RELATED_VALUES.contains(&value.to_ascii_lowercase().as_str())
    }

    fn is_type_value(property: &PropertyKind, value: &str) -> bool {
        match value.to_ascii_lowercase().as_str() {
            "work" | "home" => true,
            value => {
                is_iana_token_value(value)
                    || is_x_name_value(value)
                    || if_tel(property, value)
                    || if_related(property, value)
            }
        }
    }

    values.iter().all(|v| is_type_value(property, v))
}
