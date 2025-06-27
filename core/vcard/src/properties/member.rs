use std::collections::HashSet;

use ical::generator::Property as IcalProperty;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::values::uri::MaybeUri;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

///  To include a member in the group this vCard represents.
#[derive(Clone, Debug)]
pub struct Member {
    pub value: MaybeUri,
    pub value_type: Option<ValueType>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
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

impl Member {
    /// Create a new MEMBER property without any parameter or group
    #[must_use]
    pub fn new(value: String) -> Self {
        Self {
            value: value.into(),
            value_type: None,
            pid: None,
            preference: None,
            media_type: None,
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        }
    }
}

impl TryFrom<IcalProperty> for Member {
    type Error = VCardError;

    fn try_from(property: IcalProperty) -> VCardResult<Self> {
        let Some(value) = property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Member));
        };
        let mut result = Self::new(value);
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Member))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Member))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Member))?,
                        );
                    }
                    ParameterType::MediaType => {
                        result.media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Member))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Member))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Member))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Member,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Member {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}
