use std::collections::HashSet;
use std::fmt::Debug;

use ical::generator::Property as IcalProperty;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::values::text_list::TextList;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify application category information about the vCard, also known as "tags".
#[derive(Clone, Default, Debug)]
pub struct Category {
    /// Value
    pub value: TextList,
    /// type of the value (here nothing or "text")
    pub value_type: Option<ValueType>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// Type for this property
    pub r#type: HashSet<GenericType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this Category belong to
    pub group: Option<String>,
}

impl TryFrom<&IcalProperty> for Category {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Categories));
        };
        let mut result = Self {
            value: value.into(),
            group: group_from_name(&property.name),
            ..Default::default()
        };
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type =
                            Some(ValueType::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Categories),
                            )?);
                    }
                    ParameterType::Pid => {
                        result.pid =
                            Some(Pid::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Categories),
                            )?);
                    }
                    ParameterType::Pref => {
                        result.preference =
                            Some(Preference::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Categories),
                            )?);
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Categories))?;
                    }
                    ParameterType::AltId => {
                        result.alternative_id =
                            Some(AlternativeId::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Categories),
                            )?);
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Categories),
                            )?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Categories,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Category {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}
