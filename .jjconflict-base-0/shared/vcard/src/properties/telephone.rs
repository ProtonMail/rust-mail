use std::collections::HashSet;
use std::fmt::{Display, Formatter};

use ical::generator::Property as IcalProperty;
use url::Url;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_tel::TelType;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the telephone number for telephony communication with the object the vCard
/// represents.
#[derive(Clone, Debug)]
pub struct Telephone {
    /// Value (ex: tel:+33-01-23-45-67)
    pub value: TelephoneValue,
    /// type of the value (Uri or Text)
    pub value_type: Option<ValueType>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// Type for this property
    pub tel_type: HashSet<TelType>,
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

impl Telephone {
    /// Create a new TEL property
    #[must_use]
    pub fn new(telephone: String) -> Self {
        Self {
            value: telephone.into(),
            value_type: None,
            pid: None,
            preference: None,
            tel_type: HashSet::new(),
            media_type: None,
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        }
    }
}

impl TryFrom<&IcalProperty> for Telephone {
    type Error = VCardError;

    #[allow(clippy::too_many_lines)]
    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Tel));
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
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::Pid => {
                        pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::Pref => {
                        preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::Type => {
                        r#type = TelType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?;
                    }
                    ParameterType::MediaType => {
                        media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::AltId => {
                        alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    ParameterType::Any => {
                        any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Tel))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Tel,
                            parameter_type,
                        ));
                    }
                }
            }
        }

        Ok(Self {
            value: TelephoneValue::from(value.clone()),
            value_type,
            pid,
            preference,
            tel_type: r#type,
            media_type,
            alternative_id,
            any,
            group: group_from_name(&property.name),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TelephoneValue {
    Text(String),
    Uri(Url),
}

impl Display for TelephoneValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(v) => write!(f, "{v}"),
            Self::Uri(v) => write!(f, "{}", v.path()),
        }
    }
}

impl VcardProperty for Telephone {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

impl From<String> for TelephoneValue {
    fn from(value: String) -> Self {
        if let Ok(url) = value.parse::<Url>() {
            Self::Uri(url)
        } else {
            Self::Text(value)
        }
    }
}
