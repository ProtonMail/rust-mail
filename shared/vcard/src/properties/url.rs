use std::collections::HashSet;

use ical::generator::Property as IcalProperty;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::values::uri::MaybeUri;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify a uniform resource locator associated with the object to which the vCard refers.
/// Examples for individuals include personal websites, blogs, and social networking site
/// identifiers.
#[derive(Clone, Debug, Default)]
pub struct VcardUrl {
    /// Value (ex: <http://example.org/restaurant.french/~chezchic.html>)
    pub value: MaybeUri,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// Type for this property
    pub r#type: HashSet<GenericType>,
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

impl VcardUrl {
    /// Create a new URL property without any parameter or group
    #[must_use]
    pub fn new(value: String) -> Self {
        Self {
            value: value.into(),
            ..Default::default()
        }
    }
}

impl TryFrom<IcalProperty> for VcardUrl {
    type Error = VCardError;

    fn try_from(property: IcalProperty) -> VCardResult<Self> {
        let Some(value) = property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Url));
        };
        let mut result = Self::new(value);
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Url))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Url))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Url))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Url))?;
                    }
                    ParameterType::MediaType => {
                        result.media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Url))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Url))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Url))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Url,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for VcardUrl {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}
