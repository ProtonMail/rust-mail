use std::collections::HashSet;
use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_tel::TelType;
use crate::parameters::value::ValueType;
use crate::properties::{
    any_debug, get_value_type, loop_debug, optional_debug, validate_parameters, VcardProperty,
};
use crate::validation::get_property_kind;
use crate::values::text::{is_text_value, Text};
use crate::values::uri::{is_uri_value, Uri};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the telephone number for telephony communication with the object the vCard
/// represents.
#[derive(Clone)]
pub struct Telephone {
    /// Value (ex: tel:+33-01-23-45-67)
    pub value: TelephoneValue,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// Type for this property
    pub r#type: HashSet<TelType>,
    /// Media type linked by the value
    pub media_type: Option<MediaType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Telephone {
    /// Create a new TEL property
    #[must_use]
    pub fn new(value: TelephoneValue) -> Self {
        Self {
            value,
            value_type: None,
            pid: None,
            preference: None,
            r#type: HashSet::new(),
            media_type: None,
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new TEL property
    ///
    /// # Errors
    ///   * if the value is not a text value or an uri value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self::new(TelephoneValue::try_from(value)?))
    }
}

impl Debug for Telephone {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Telephone {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        optional_debug!(self, f, PID, pid);
        optional_debug!(self, f, PREF, preference);
        loop_debug!(self, f, TYPE, r#type);
        optional_debug!(self, f, MEDIATYPE, media_type);
        optional_debug!(self, f, ALTID, alternative_id);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}")
    }
}

impl TryFrom<&IcalProperty> for Telephone {
    type Error = VCardError;

    #[allow(clippy::too_many_lines)]
    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Tel));
        };
        let mut value_type = None;
        let mut pid = None;
        let mut preference = None;
        let mut r#type = HashSet::new();
        let mut media_type = None;
        let mut alternative_id = None;
        let mut any = HashSet::new();
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::Pid => {
                        pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::Pref => {
                        preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::Type => {
                        r#type = TelType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?;
                    }
                    ParameterType::MediaType => {
                        media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::AltId => {
                        alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::Any => {
                        any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Tel,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        let real_value_type = if let Some(value_type) = value_type {
            value_type
        } else if is_uri_value(value) {
            ValueType::Uri
        } else if is_text_value(value) {
            ValueType::Text
        } else {
            return Err(VCardError::InvalidValue(
                PropertyKind::Tel,
                value.to_owned(),
            ));
        };
        let value = match real_value_type {
            ValueType::Text => TelephoneValue::Text(
                Text::try_from(value.as_str())
                    .map_err(VCardError::from_value_error(PropertyKind::Tel))?,
            ),
            ValueType::Uri => TelephoneValue::Uri(
                Uri::try_from(value.as_str())
                    .map_err(VCardError::from_value_error(PropertyKind::Tel))?,
            ),
            _ => {
                return Err(VCardError::InvalidValue(
                    PropertyKind::Tel,
                    value.to_owned(),
                ));
            }
        };
        Ok(Self {
            value,
            value_type,
            pid,
            preference,
            r#type,
            media_type,
            alternative_id,
            any,
            group: group_from_name(&property.name),
        })
    }
}

impl VcardProperty for Telephone {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

#[derive(Clone, PartialEq)]
pub enum TelephoneValue {
    Text(Text),
    Uri(Uri),
}

impl Debug for TelephoneValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(v) => write!(f, "{v:?}"),
            Self::Uri(v) => write!(f, "{v:?}"),
        }
    }
}

impl TryFrom<&str> for TelephoneValue {
    type Error = VCardError;

    fn try_from(value: &str) -> VCardResult<Self> {
        if let Ok(url) = value.parse() {
            Ok(Self::Uri(Uri::new(url)))
        } else if is_text_value(value) {
            Ok(Self::Text(Text::new_unchecked(value)))
        } else {
            Err(VCardError::InvalidValue(
                PropertyKind::Tel,
                value.to_owned(),
            ))
        }
    }
}

/// Validate that the given `property` respect the format for a `TEL` property
///
/// # Errors
///   * if the property value is not a text value nor an uri value
///   * if any of the parameters is invalid
#[allow(clippy::too_many_lines)]
pub fn validate_tel(property: &IcalProperty) -> VcardValidationResult<()> {
    // TEL-param = TEL-text-param / TEL-uri-param
    // TEL-value = TEL-text-value / TEL-uri-value
    //   ; Value and parameter MUST match.
    //
    // TEL-text-param = "VALUE=text"
    // TEL-text-value = text
    //
    // TEL-uri-param = "VALUE=uri" / mediatype-param
    // TEL-uri-value = URI
    //
    // TEL-param =/ type-param / pid-param / pref-param / altid-param / any-param
    //
    // type-param-tel = "text" / "voice" / "fax" / "cell" / "video" / "pager" / "textphone" / iana-token / x-name
    //   ; type-param-tel MUST NOT be used with a property other than TEL.
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

        let allowed = match value_type {
            ValueType::Text => hash_set!(
                ParameterType::Value,
                ParameterType::Type,
                ParameterType::Pid,
                ParameterType::Pref,
                ParameterType::AltId,
                ParameterType::Any
            ),
            ValueType::Uri => hash_set!(
                ParameterType::Value,
                ParameterType::MediaType,
                ParameterType::Type,
                ParameterType::Pid,
                ParameterType::Pref,
                ParameterType::AltId,
                ParameterType::Any
            ),
            _ => {
                return Err(VcardValidationError::InvalidPropertyValue(
                    get_property_kind(&property.name)?,
                ));
            }
        };

        validate_parameters(property, value_type, &allowed)?;
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
