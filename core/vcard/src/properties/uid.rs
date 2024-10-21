use std::collections::HashSet;
use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::any::Any;
use crate::parameters::preference::Preference;
use crate::parameters::value::ValueType;
use crate::properties::{
    any_debug, get_value_type, optional_debug, validate_parameters, VcardProperty,
};
use crate::validation::get_property_kind;
use crate::values::text::{is_text_value, Text};
use crate::values::uri::{is_uri_value, Uri};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify a value that represents a globally unique identifier corresponding to the entity
/// associated with the vCard.
#[derive(Clone)]
pub struct VcardUid {
    /// Value (ex: urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6)
    pub value: UidValue,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl VcardUid {
    /// Create a new UID property (no check is done)
    #[must_use]
    pub fn new(value: UidValue) -> Self {
        Self {
            value,
            value_type: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new UID property
    ///
    /// # Errors
    ///   * if given value is not a valid uri
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self::new(UidValue::try_from(value)?))
    }
}

impl Debug for VcardUid {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Uid {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}",)
    }
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
                        ))
                    }
                }
            }
        };
        let real_value_type = if let Some(value_type) = value_type {
            value_type
        } else if is_uri_value(value) {
            ValueType::Uri
        } else if is_text_value(value) {
            ValueType::Text
        } else {
            return Err(VCardError::InvalidValue(
                PropertyKind::UId,
                value.to_owned(),
            ));
        };
        let value = match real_value_type {
            ValueType::Text => UidValue::Text(
                Text::try_from(value.as_str())
                    .map_err(VCardError::from_value_error(PropertyKind::UId))?,
            ),
            ValueType::Uri => UidValue::Uri(
                Uri::try_from(value.as_str())
                    .map_err(VCardError::from_value_error(PropertyKind::UId))?,
            ),
            _ => {
                return Err(VCardError::InvalidValue(
                    PropertyKind::UId,
                    value.to_owned(),
                ))
            }
        };
        Ok(Self {
            value,
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

#[derive(Clone, PartialEq)]
pub enum UidValue {
    Text(Text),
    Uri(Uri),
}

impl Debug for UidValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UidValue::Text(v) => write!(f, "{v:?}"),
            UidValue::Uri(v) => write!(f, "{v:?}"),
        }
    }
}

impl TryFrom<&str> for UidValue {
    type Error = VCardError;

    fn try_from(value: &str) -> VCardResult<Self> {
        if let Ok(value) = value.parse() {
            Ok(Self::Uri(Uri::new(value)))
        } else if is_text_value(value) {
            Ok(Self::Text(Text::new_unchecked(value)))
        } else {
            Err(VCardError::InvalidValue(
                PropertyKind::UId,
                value.to_owned(),
            ))
        }
    }
}

/// Validate that the given `property` respect the format for a `UID` property
///
/// # Errors
///   * property value is not a text neither an uri
///   * any of the parameters is not valid
pub fn validate_uid(property: &IcalProperty) -> VcardValidationResult<()> {
    // UID-param = UID-uri-param / UID-text-param
    // UID-value = UID-uri-value / UID-text-value
    //   ; Value and parameter MUST match.
    //
    // UID-uri-param = "VALUE=uri"
    // UID-uri-value = URI
    //
    // UID-text-param = "VALUE=text"
    // UID-text-value = text
    //
    // UID-param =/ any-param
    if let Some(value) = &property.value {
        let value_type = if let Some(value_type) = get_value_type(property)? {
            let validated = match value_type {
                ValueType::Text => is_text_value(value),
                ValueType::Uri => is_uri_value(value),
                _ => false,
            };
            if !validated {
                return Err(VcardValidationError::InvalidPropertyValue(
                    get_property_kind(&property.name)?,
                ));
            }
            value_type
        } else if is_text_value(value) {
            ValueType::Text
        } else if is_uri_value(value) {
            ValueType::Uri
        } else {
            return Err(VcardValidationError::InvalidPropertyValue(
                get_property_kind(&property.name)?,
            ));
        };
        validate_parameters(
            property,
            value_type,
            &hash_set!(ParameterType::Value, ParameterType::Any,),
        )?;
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
