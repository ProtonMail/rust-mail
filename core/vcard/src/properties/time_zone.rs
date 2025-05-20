use std::collections::HashSet;
use std::fmt::{Debug, Display};

use ical::generator::Property as IcalProperty;
use url::Url;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::{VcardProperty, get_value_type, validate_parameters};
use crate::validation::get_property_kind;
use crate::values::text::Text;
use crate::values::uri::Uri;
use crate::values::utc_offset::UTCOffset;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify information related to the time zone of the object the vCard represents.
#[derive(Clone, Debug)]
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

        Ok(Self {
            value: value.as_str().into(),
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

#[derive(Clone, Debug, PartialEq)]
pub enum TimeZoneValue {
    Text(Text),
    Uri(Uri),
    UtcOffset(UTCOffset),
}

impl From<&str> for TimeZoneValue {
    fn from(value: &str) -> Self {
        if let Ok(url) = Url::parse(value) {
            TimeZoneValue::Uri(Uri(url))
        } else if let Ok(offset) = UTCOffset::try_from(value) {
            TimeZoneValue::UtcOffset(offset)
        } else {
            TimeZoneValue::Text(value.into())
        }
    }
}

impl Display for TimeZoneValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeZoneValue::Text(text) => write!(f, "{}", text.value),
            TimeZoneValue::Uri(uri) => write!(f, "{}", uri.0),
            TimeZoneValue::UtcOffset(offset) => write!(f, "{offset}"),
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
    if property.value.is_some() {
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
        } else {
            ValueType::Text
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
