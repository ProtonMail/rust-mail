use crate::errors::{VCardParameterError, VCardParameterResult};
use std::collections::HashSet;

use crate::ParameterType;
use crate::values::iana_token::{IanaToken, is_iana_token_value};
use crate::values::x_name::{XName, is_x_name_value};

// /// The TYPE parameter has multiple, different uses.  In general, it is a way of specifying class
// /// characteristics of the associated property.
// type-param-tel = "text" / "voice" / "fax" / "cell" / "video" / "pager" / "textphone" / iana-token / x-name
pub(super) const TEL_VALUES: [&str; 7] = [
    "text",
    "voice",
    "fax",
    "cell",
    "video",
    "pager",
    "textphone",
];

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TelType {
    Home,
    Work,
    Text,
    Voice,
    Fax,
    Cell,
    Video,
    Pager,
    TextPhone,
    IanaToken(IanaToken),
    XName(XName),
}

impl TelType {
    /// Try to create a new TYPE parameter for Telephone property
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Self::try_from(value)
    }

    /// Try to create a new `HashSet` of TYPE parameters
    pub fn set_from_values(values: &[String]) -> VCardParameterResult<HashSet<Self>> {
        values.iter().map(|v| Self::try_from(v.as_str())).collect()
    }
}

impl TryFrom<&str> for TelType {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        match value.to_ascii_lowercase().as_ref() {
            "home" => Ok(Self::Home),
            "work" => Ok(Self::Work),
            "text" => Ok(Self::Text),
            "voice" => Ok(Self::Voice),
            "fax" => Ok(Self::Fax),
            "cell" => Ok(Self::Cell),
            "video" => Ok(Self::Video),
            "pager" => Ok(Self::Pager),
            "textphone" => Ok(Self::TextPhone),
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
