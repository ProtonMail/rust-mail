use std::collections::HashSet;
use std::fmt::Debug;

use ical::generator::Property as IcalProperty;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::calendar_scale::CalendarScale;
use crate::parameters::preference::Preference;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::values::date_and_or_time::{DateAndOrTimeValue, MaybeDateAndOrTime};
use crate::values::text::Text;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// The date of marriage, or equivalent, of the object the vCard represents.
#[derive(Debug, Clone, Default)]
pub struct Anniversary {
    /// Value (ex: 19960415)
    pub value: MaybeDateAndOrTime,
    /// type of the value (here nothing or "date-and-or-time" of "text")
    pub value_type: Option<ValueType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Calendar scale
    pub calendar_scale: Option<CalendarScale>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Anniversary {
    /// Try to create a new ANNIVERSARY property
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: value.into(),
            value_type: None,
            alternative_id: None,
            calendar_scale: None,
            any: HashSet::new(),
            group: None,
        })
    }
}

impl TryFrom<&IcalProperty> for Anniversary {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Anniversary));
        };
        let mut value_type = None;
        let mut alternative_id = None;
        let mut calendar_scale = None;
        let mut any = HashSet::new();
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        value_type = Some(ValueType::try_from(values.as_slice()).map_err(
                            VCardError::from_parameter_error(PropertyKind::Anniversary),
                        )?);
                    }
                    ParameterType::AltId => {
                        alternative_id = Some(AlternativeId::try_from(values.as_slice()).map_err(
                            VCardError::from_parameter_error(PropertyKind::Anniversary),
                        )?);
                    }
                    ParameterType::CalScale => {
                        calendar_scale = Some(CalendarScale::try_from(values.as_slice()).map_err(
                            VCardError::from_parameter_error(PropertyKind::Anniversary),
                        )?);
                    }
                    ParameterType::Any => {
                        any.insert(
                            Any::new_validated(name.as_str(), values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Anniversary),
                            )?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Anniversary,
                            parameter_type,
                        ));
                    }
                }
            }
        }

        Ok(Self {
            value: value.into(),
            value_type,
            alternative_id,
            calendar_scale,
            any,
            group: group_from_name(&property.name),
        })
    }
}

impl VcardProperty for Anniversary {
    fn get_preference(&self) -> Option<Preference> {
        None
    }
}
#[derive(Clone, PartialEq)]
pub enum AnniversaryValue {
    DateAndOrTime(DateAndOrTimeValue),
    Text(Text),
}

impl AnniversaryValue {
    /// Try to create a new value for ANNIVERSARY property
    pub fn new_validated(value: impl AsRef<str>) -> Self {
        Self::from(value)
    }
}

impl<T: AsRef<str>> From<T> for AnniversaryValue {
    fn from(value: T) -> Self {
        match DateAndOrTimeValue::try_from(value.as_ref()) {
            Ok(v) => Self::DateAndOrTime(v),
            _ => Self::Text(value.into()),
        }
    }
}
