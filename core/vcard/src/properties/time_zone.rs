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
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::{
    VcardProperty, any_debug, get_value_type, loop_debug, optional_debug, validate_parameters,
};
use crate::validation::get_property_kind;
use crate::values::text::{Text, is_text_value};
use crate::values::uri::{Uri, is_uri_value};
use crate::values::utc_offset::{UTCOffset, is_utc_offset_value};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify information related to the time zone of the object the vCard represents.
#[derive(Clone)]
pub struct TimeZone {
    /// Value (ex: -0500 or Raleigh/North America)
    pub value: TimeZoneValue,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// Type for this property
    pub r#type: HashSet<GenericType>,
    /// Media type linked by the value
    pub media_type: Option<MediaType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl TimeZone {
    /// Create a new TZ property without any parameter or group
    #[must_use]
    pub fn new(value: TimeZoneValue) -> Self {
        Self {
            value,
            value_type: None,
            alternative_id: None,
            pid: None,
            preference: None,
            r#type: HashSet::new(),
            media_type: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new TZ property without any parameter or group
    ///
    /// # Errors
    ///   * if the given value not a valid text, uri or utc-offset
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self::new(TimeZoneValue::try_from(value)?))
    }
}

impl Debug for TimeZone {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "TimeZone {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        optional_debug!(self, f, PID, pid);
        optional_debug!(self, f, PREF, preference);
        loop_debug!(self, f, TYPE, r#type);
        optional_debug!(self, f, MEDIATYPE, media_type);
        optional_debug!(self, f, ALTID, alternative_id);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}",)
    }
}

impl TryFrom<&IcalProperty> for TimeZone {
    type Error = VCardError;

    #[allow(clippy::too_many_lines)]
    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Tz));
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
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tz))?,
                        );
                    }
                    ParameterType::Pid => {
                        pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tz))?,
                        );
                    }
                    ParameterType::Pref => {
                        preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tz))?,
                        );
                    }
                    ParameterType::Type => {
                        r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Tz))?;
                    }
                    ParameterType::MediaType => {
                        media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tz))?,
                        );
                    }
                    ParameterType::AltId => {
                        alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tz))?,
                        );
                    }
                    ParameterType::Any => {
                        any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tz))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Tz,
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
        } else if is_utc_offset_value(value) {
            ValueType::UTCOffset
        } else if is_text_value(value) {
            ValueType::Text
        } else {
            return Err(VCardError::InvalidValue(PropertyKind::Tz, value.to_owned()));
        };
        let value = match real_value_type {
            ValueType::Text => TimeZoneValue::Text(
                Text::try_from(value.as_str())
                    .map_err(VCardError::from_value_error(PropertyKind::Tz))?,
            ),
            ValueType::Uri => TimeZoneValue::Uri(
                Uri::try_from(value.as_str())
                    .map_err(VCardError::from_value_error(PropertyKind::Tz))?,
            ),
            ValueType::UTCOffset => TimeZoneValue::UtcOffset(
                UTCOffset::try_from(value.as_str())
                    .map_err(VCardError::from_value_error(PropertyKind::Tz))?,
            ),
            _ => return Err(VCardError::InvalidValue(PropertyKind::Tz, value.to_owned())),
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

impl VcardProperty for TimeZone {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

#[derive(Clone, PartialEq)]
pub enum TimeZoneValue {
    Text(Text),
    Uri(Uri),
    UtcOffset(UTCOffset),
}

impl TryFrom<&str> for TimeZoneValue {
    type Error = VCardError;

    fn try_from(value: &str) -> VCardResult<Self> {
        if let Ok(value) = value.parse() {
            Ok(Self::Uri(Uri::new(value)))
        } else if is_utc_offset_value(value) {
            Ok(Self::UtcOffset(
                value
                    .try_into()
                    .map_err(VCardError::from_value_error(PropertyKind::Tz))?,
            ))
        } else if is_text_value(value) {
            Ok(Self::Text(Text::new_unchecked(value)))
        } else {
            Err(VCardError::InvalidValue(PropertyKind::Tz, value.to_owned()))
        }
    }
}

impl Debug for TimeZoneValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(v) => write!(f, "{v:?}"),
            Self::Uri(v) => write!(f, "{v:?}"),
            Self::UtcOffset(v) => write!(f, "{v:?}"),
        }
    }
}

/// Validate that the given `property` respect the format for a `TZ` property
///
/// # Errors
///   * if property value is not a valid text value, an uri value or an utc-offset
///   * if any of the parameters is not valid
pub fn validate_tz(property: &IcalProperty) -> VcardValidationResult<()> {
    // TZ-param = "VALUE=" ("text" / "uri" / "utc-offset")
    // TZ-value = text / URI / utc-offset
    //   ; Value and parameter MUST match.
    //
    // TZ-param =/ altid-param / pid-param / pref-param / type-param / mediatype-param / any-param
    if let Some(value) = &property.value {
        let value_type = if let Some(value_type) = get_value_type(property)? {
            if matches!(
                value_type,
                ValueType::Uri | ValueType::Text | ValueType::UTCOffset
            ) {
                value_type
            } else {
                return Err(VcardValidationError::InvalidPropertyValue(
                    get_property_kind(&property.name)?,
                ));
            }
        } else if is_text_value(value) {
            ValueType::Text
        } else if is_uri_value(value) {
            ValueType::Uri
        } else if is_utc_offset_value(value) {
            ValueType::UTCOffset
        } else {
            return Err(VcardValidationError::InvalidPropertyValue(
                get_property_kind(&property.name)?,
            ));
        };
        validate_parameters(
            property,
            value_type,
            &hash_set!(
                ParameterType::Value,
                ParameterType::AltId,
                ParameterType::Pid,
                ParameterType::Pref,
                ParameterType::Type,
                ParameterType::MediaType,
                ParameterType::Any
            ),
        )?;
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
