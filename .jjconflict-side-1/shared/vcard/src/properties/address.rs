use std::collections::HashSet;

use anyhow::Context as _;
use ical::generator::Property as IcalProperty;
use tracing::warn;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::geo_localisation::GeoLocalisation;
use crate::parameters::label::Label;
use crate::parameters::language::Language;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::time_zone::TimeZone;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::values::list_component::ListComponent;
use crate::vcard::{group_from_name, split_list};
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the components of the delivery address for the vCard object.
#[derive(Clone, Debug, Default)]
pub struct Address {
    pub post_office_box: ListComponent,
    /// E.g apartment, suite, unit, building, floor, etc
    pub extended_address: ListComponent,
    pub street: ListComponent,
    /// AKA City
    pub locality: ListComponent,
    /// State or Province
    pub region: ListComponent,
    pub postal_code: ListComponent,
    pub country: ListComponent,

    pub value_type: Option<ValueType>,
    pub label: Option<Label>,
    pub language: Option<Language>,
    pub geo_localisation: Option<GeoLocalisation>,
    pub time_zone: Option<TimeZone>,
    pub alternative_id: Option<AlternativeId>,
    pub pid: Option<Pid>,
    pub preference: Option<Preference>,
    pub r#type: HashSet<GenericType>,
    pub any: HashSet<Any>,
    pub group: Option<String>,
}

impl TryFrom<IcalProperty> for Address {
    type Error = VCardError;

    #[allow(clippy::too_many_lines)]
    fn try_from(property: IcalProperty) -> VCardResult<Self> {
        let Some(value) = property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Adr));
        };

        // ADR-value = ADR-component-pobox ";" ADR-component-ext ";" ADR-component-street ";" ADR-component-locality ";" ADR-component-region ";" ADR-component-code ";" ADR-component-country
        // ADR-component-pobox    = list-component
        // ADR-component-ext      = list-component
        // ADR-component-street   = list-component
        // ADR-component-locality = list-component
        // ADR-component-region   = list-component
        // ADR-component-code     = list-component
        // ADR-component-country  = list-component
        // So a valid ADR value can be ';;;;;;' => 7 empty list-component

        let mut values = split_list(&value, ';').into_iter();
        let mut next = || {
            values
                .next()
                .context("Too little args in Adr")
                .map(|x| ListComponent::try_from(&*x).unwrap_or_default())
        };

        let mut result = Self {
            post_office_box: next()?,
            extended_address: next()?,
            street: next()?,
            locality: next()?,
            region: next()?,
            postal_code: next()?,
            country: next()?,
            group: group_from_name(property.name.as_str()),
            ..Default::default()
        };

        if let Some(parameters) = property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?,
                        );
                    }
                    ParameterType::Label => {
                        result.label = Some(
                            Label::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?,
                        );
                    }
                    ParameterType::Language => {
                        result.language = Some(
                            Language::try_from(values.clone())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?,
                        );
                    }
                    ParameterType::Geo => {
                        result.geo_localisation = Some(
                            GeoLocalisation::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?,
                        );
                    }
                    ParameterType::TZ => {
                        result.time_zone = Some(
                            TimeZone::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?;
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Adr))?,
                        );
                    }
                    parameter_type => {
                        warn!("Unexpected parameter: {parameter_type:?}");
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Address {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}
