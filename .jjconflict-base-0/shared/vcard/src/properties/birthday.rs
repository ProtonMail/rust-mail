use std::collections::HashSet;
use std::fmt::Debug;

use ical::generator::Property as IcalProperty;
use tracing::warn;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::calendar_scale::CalendarScale;
use crate::parameters::language::Language;
use crate::parameters::value::ValueType;
use crate::values::date_and_or_time::MaybeDateAndOrTime;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the birthdate of the object the vCard represents.
#[derive(Clone, Default, Debug)]
pub struct Birthday {
    /// Value (ex: --0415 or circa 1800)
    pub value: MaybeDateAndOrTime,
    /// type of the value (here nothing or "date-and-or-time" of "text")
    pub value_type: Option<ValueType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Calendar scale
    pub calendar_scale: Option<CalendarScale>,
    /// Language
    pub language: Option<Language>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Birthday {
    /// Try to create a new BDAY property
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: value.into(),
            ..Default::default()
        })
    }
}

impl TryFrom<&IcalProperty> for Birthday {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::BDay));
        };
        let mut value_type = None;
        let mut alternative_id = None;
        let mut calendar_scale = None;
        let mut language = None;
        let mut any = HashSet::new();
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    ParameterType::AltId => {
                        alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    ParameterType::CalScale => {
                        calendar_scale = Some(
                            CalendarScale::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    ParameterType::Language => {
                        language = Some(
                            Language::try_from(values.clone())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    ParameterType::Any => {
                        any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    parameter_type => {
                        warn!("Unexpected parameter: {parameter_type:?}");
                    }
                }
            }
        }

        Ok(Self {
            value: value.into(),
            value_type,
            alternative_id,
            calendar_scale,
            language,
            any,
            group: group_from_name(&property.name),
        })
    }
}
