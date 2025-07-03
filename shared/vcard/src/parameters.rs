//! Structure grouping all parameter of a vCard property
//!
//! No check here on parameter valid for the corresponding property

use std::collections::HashSet;
use std::fmt::Debug;

use ical::generator::Property as IcalProperty;

use crate::PropertyKind;
use crate::errors::{VCardParameterError, VCardParameterResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::calendar_scale::CalendarScale;
use crate::parameters::geo_localisation::GeoLocalisation;
use crate::parameters::label::Label;
use crate::parameters::language::Language;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::sort_as::SortAs;
use crate::parameters::time_zone::TimeZone;
use crate::parameters::type_generic::GenericType;
use crate::parameters::type_related::RelatedType;
use crate::parameters::type_tel::TelType;
use crate::parameters::value::ValueType;
use crate::validation::get_property_kind;
use crate::values::param_value::ParamValue;

pub mod alternative_id;
pub mod any;
pub mod calendar_scale;
pub mod geo_localisation;
pub mod label;
pub mod language;
pub mod mediatype;
pub mod pid;
pub mod preference;
pub mod sort_as;
pub mod time_zone;
pub mod type_generic;
pub mod type_related;
pub mod type_tel;
pub mod value;

/// Structure grouping all parameters of a vCard property
#[derive(Debug, Clone, Default)]
pub struct Parameters {
    pub alternative_id: Option<AlternativeId>,
    pub any: HashSet<Any>,
    pub calendar_scale: Option<CalendarScale>,
    pub geo_localisation: Option<GeoLocalisation>,
    pub label: Option<Label>,
    pub language: Option<Language>,
    pub media_type: Option<MediaType>,
    pub pid: Option<Pid>,
    pub preference: Option<Preference>,
    pub sort_as: Option<SortAs>,
    // TODO: find a way to merge all 3 types without loosing specialisation
    pub generic_types: HashSet<GenericType>,
    pub tel_types: HashSet<TelType>,
    pub related_types: HashSet<RelatedType>,
    pub time_zone: Option<TimeZone>,
    pub value: Option<ValueType>,
}

macro_rules! set_handler {
    ($name:ident, $plural:ident, $type:ty) => {
        paste::paste! {
            #[doc = "Add a "]
            #[doc = stringify!($name)]
            #[doc = " parameter"]
            #[must_use] pub fn [<add_ $name>](mut self, values: &HashSet<$type>) -> Self{
                self.$plural = &self.$plural | values;
                self
            }
        }
    };
}

macro_rules! single_handler {
    ($name:ident, $type:ty) => {
        paste::paste! {
            #[doc = "Set a "]
            #[doc = stringify!($name)]
            #[doc = " parameter (chainable)"]
            #[must_use] pub fn [<with_ $name>](mut self, value: $type) -> Self {
                self.$name = Some(value.into());
                self
            }
        }
    };
}

impl Parameters {
    /// Create a new empty set of parameters
    pub(crate) fn new() -> Self {
        Self::default()
    }

    single_handler!(alternative_id, ParamValue);
    set_handler!(any, any, Any);
    single_handler!(calendar_scale, CalendarScale);
    single_handler!(geo_localisation, GeoLocalisation);
    single_handler!(label, Label);
    single_handler!(language, Language);
    single_handler!(media_type, MediaType);
    single_handler!(pid, Pid);
    single_handler!(preference, u32);
    single_handler!(sort_as, SortAs);
    set_handler!(generic_type, generic_types, GenericType);
    set_handler!(tel_type, tel_types, TelType);
    set_handler!(related_type, related_types, RelatedType);
    single_handler!(time_zone, TimeZone);
    single_handler!(value, ValueType);
}

impl TryFrom<&IcalProperty> for Parameters {
    type Error = VCardParameterError;

    fn try_from(property: &IcalProperty) -> VCardParameterResult<Self> {
        let mut result = Self::new();
        let Some(parameters) = &property.params else {
            return Ok(result);
        };
        for (name, values) in parameters {
            match ParameterType::from(name.as_str()) {
                ParameterType::AltId => {
                    result.alternative_id = Some(AlternativeId::try_from(values.as_slice())?);
                }
                ParameterType::Any => {
                    result
                        .any
                        .insert(Any::new_validated(&property.name, values)?);
                }
                ParameterType::CalScale => {
                    result.calendar_scale = Some(CalendarScale::try_from(values.as_slice())?);
                }
                ParameterType::Geo => {
                    result.geo_localisation = Some(GeoLocalisation::try_from(values.as_slice())?);
                }
                ParameterType::Label => {
                    result.label = Some(Label::try_from(values.as_slice())?);
                }
                ParameterType::Language => {
                    result.language = Some(Language::try_from(values.clone())?);
                }
                ParameterType::MediaType => {
                    result.media_type = Some(MediaType::try_from(values.as_slice())?);
                }
                ParameterType::Pid => result.pid = Some(Pid::try_from(values.as_slice())?),
                ParameterType::Pref => {
                    result.preference = Some(Preference::try_from(values.as_slice())?);
                }
                ParameterType::SortAs => {
                    result.sort_as = Some(SortAs::try_from(values.as_slice())?);
                }
                ParameterType::Type => match get_property_kind(property.name.as_str())
                    .map_err(|_| VCardParameterError::InvalidPropertyName(property.name.clone()))?
                {
                    PropertyKind::Tel => {
                        result = result.add_tel_type(&TelType::set_from_values(values)?);
                    }
                    PropertyKind::Related => {
                        result = result.add_related_type(&RelatedType::set_from_values(values)?);
                    }

                    _ => result = result.add_generic_type(&GenericType::set_from_values(values)?),
                },
                ParameterType::TZ => {
                    result.time_zone = Some(TimeZone::try_from(values.as_slice())?);
                }
                ParameterType::Value => {
                    result.value = Some(ValueType::try_from(values.as_slice())?);
                }
            }
        }
        Ok(result)
    }
}

/// All possible type for parameters
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ParameterType {
    /// Alternative Id
    AltId,
    /// Any additional parameter
    Any,
    /// Calendar system
    CalScale,
    /// Geo localisation
    Geo,
    /// Delivery address label
    Label,
    /// Define the language used in property
    Language,
    /// Media type for associated URI
    MediaType,
    /// Property Id
    Pid,
    /// Preference between identical property
    Pref,
    /// Specific sorting configuration
    SortAs,
    /// Tag-like parameter to define categories
    Type,
    /// Time Zone
    TZ,
    /// Type of the Property value
    Value,
}

impl From<&str> for ParameterType {
    fn from(value: &str) -> Self {
        match value.to_ascii_uppercase().as_str() {
            "VALUE" => Self::Value,
            "ALTID" => Self::AltId,
            "CALSCALE" => Self::CalScale,
            "GEO" => Self::Geo,
            "LABEL" => Self::Label,
            "LANGUAGE" => Self::Language,
            "MEDIATYPE" => Self::MediaType,
            "PID" => Self::Pid,
            "PREF" => Self::Pref,
            "SORT-AS" => Self::SortAs,
            "TYPE" => Self::Type,
            "TZ" => Self::TZ,
            _ => Self::Any,
        }
    }
}
