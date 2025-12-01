use std::collections::HashSet;

use ical::generator::Property as IcalProperty;
use url::Url;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::values::uri::Uri;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the calendar user address (RFC5545) to which a scheduling request (RFC5546) should be
/// sent for the object represented by the vCard.
#[derive(Clone, Debug)]
pub struct CalendarUserAddress {
    /// Value (ex: <http://example.com/calendar/jdoe>, <mailto:janedoe@example.com>)
    pub value: Uri,
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

impl CalendarUserAddress {
    /// Create a new CALADRURI property without parameter or group
    #[must_use]
    pub fn new(value: Url) -> Self {
        Self {
            value: Uri::new(value),
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

    /// Try to create a new `CalAdrUri` property from given value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self::new(value.parse().map_err(|_| {
            VCardError::InvalidValue(PropertyKind::CalAdrURI, value.to_owned())
        })?))
    }
}

impl TryFrom<&IcalProperty> for CalendarUserAddress {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::CalAdrURI));
        };
        let mut result = Self {
            value: Uri::try_from(value.as_str())
                .map_err(VCardError::from_value_error(PropertyKind::CalAdrURI))?,
            value_type: None,
            pid: None,
            preference: None,
            r#type: HashSet::new(),
            media_type: None,
            alternative_id: None,
            any: HashSet::new(),
            group: group_from_name(&property.name),
        };
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type =
                            Some(ValueType::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::CalAdrURI),
                            )?);
                    }
                    ParameterType::Pid => {
                        result.pid =
                            Some(Pid::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::CalAdrURI),
                            )?);
                    }
                    ParameterType::Pref => {
                        result.preference =
                            Some(Preference::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::CalAdrURI),
                            )?);
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::CalAdrURI))?;
                    }
                    ParameterType::MediaType => {
                        result.media_type =
                            Some(MediaType::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::CalAdrURI),
                            )?);
                    }
                    ParameterType::AltId => {
                        result.alternative_id =
                            Some(AlternativeId::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::CalAdrURI),
                            )?);
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::CalAdrURI),
                            )?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::CalAdrURI,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for CalendarUserAddress {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}
