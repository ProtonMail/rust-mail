#![allow(clippy::module_name_repetitions)]
//! This module provide features to validate vCards and/or its constituents (Properties, Values and parameters).
//!
//! This crate is based around iCal crate  <https://docs.rs/ical/latest/ical/>, all parsing is done with it.
//! As iCal doesn't do validation of the values (i.e. Value, parameters and properties), this crate provide functions to validate them.
//!
//! See RFC6350: vCard Format Specification <https://www.rfc-editor.org/rfc/rfc6350> for more details.
//!
//! # Typical usages
//!
use crate::parameters::alternative_id::is_altid_param;
use crate::parameters::any::is_any_param;
use crate::parameters::calendar_scale::is_calscale_param;
use crate::parameters::geo_localisation::is_geo_param;
use crate::parameters::label::is_label_param;
use crate::parameters::language::is_language_param;
use crate::parameters::mediatype::is_mediatype_param;

mod errors;
pub mod parameters;
pub mod properties;
#[cfg(test)]
mod test;
mod validation;
pub mod values;
pub mod vcard;

use ::url::Url;
pub use properties::*;

pub use crate::errors::VCardError;
pub use crate::errors::VCardResult;
pub use crate::errors::VcardValidationError;
pub use crate::parameters::ParameterType;
use crate::parameters::pid::is_pid_param;
use crate::parameters::preference::is_pref_param;
use crate::parameters::sort_as::is_sort_as_param;
use crate::parameters::time_zone::is_tz_param;
use crate::parameters::type_generic::is_type_param;
pub use crate::parameters::value::ValueType;
pub use crate::properties::PropertyKind;
use crate::values::component::is_component_value;
use crate::values::date::is_date_value;
use crate::values::date_and_or_time::is_date_and_or_time_value;
use crate::values::date_time::is_date_time_value;
use crate::values::iana_token::is_iana_token_value;
use crate::values::list_component::is_list_component_value;
use crate::values::param_value::is_param_value;
use crate::values::time::is_time_value;
use crate::values::timestamp::is_timestamp_value;
use crate::values::utc_offset::is_utc_offset_value;
use crate::values::x_name::is_x_name_value;
use crate::values::zone::is_zone_value;

/// Validate that the given value is valid for the given `ValueType`
#[must_use]
pub fn is_value_type(kind: ValueType, value: &str) -> bool {
    match kind {
        ValueType::Text | ValueType::LanguageTag | ValueType::TextList => true,
        ValueType::Component => is_component_value(value),
        ValueType::Date => is_date_value(value),
        ValueType::DateAndOrTime => is_date_and_or_time_value(value),
        ValueType::DateTime => is_date_time_value(value),
        ValueType::IanaToken => is_iana_token_value(value),
        ValueType::ListComponent => is_list_component_value(value),
        ValueType::ParamValue => is_param_value(value),
        ValueType::Time => is_time_value(value),
        ValueType::Timestamp => is_timestamp_value(value),
        ValueType::Uri => Url::parse(value).is_ok(),
        ValueType::UTCOffset => is_utc_offset_value(value),
        ValueType::XName => is_x_name_value(value),
        ValueType::TimeZone => is_zone_value(value),
    }
}

/// Validate the given values are valid for the given `ParameterType`
///
/// To validate an `any` parameter including its name, use `is_any_param` directly.
/// To validate a `TYPE` parameter for a particular property, use `is_type_param` directly (`TEL` and `RELATED` have specific possible values)
#[must_use]
pub fn is_valid_parameter(kind: &ParameterType, values: &[String]) -> bool {
    match kind {
        ParameterType::AltId => is_altid_param(values),
        // We are not validating the name of the parameter, but only its value
        ParameterType::Any => is_any_param("iana-token", values),
        ParameterType::CalScale => is_calscale_param(values),
        ParameterType::Geo => is_geo_param(values),
        ParameterType::Label => is_label_param(values),
        ParameterType::Language => is_language_param(values),
        ParameterType::MediaType => is_mediatype_param(values),
        ParameterType::Pid => is_pid_param(values),
        ParameterType::Pref => is_pref_param(values),
        ParameterType::SortAs => is_sort_as_param(values),
        // We are not validating `TYPE` for any specific property
        ParameterType::Type => is_type_param(&PropertyKind::Adr, values),
        ParameterType::TZ => is_tz_param(values),
        ParameterType::Value => {
            values.len() == 1 && ValueType::try_from(values[0].as_str()).is_ok()
        }
    }
}
