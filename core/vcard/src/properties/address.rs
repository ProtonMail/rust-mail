use std::collections::HashSet;
use std::mem;

use ical::generator::Property as IcalProperty;
use tracing::warn;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
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
use crate::properties::{VcardProperty, validate_parameters};
use crate::validation::get_property_kind;
use crate::values::check_list;
use crate::values::list_component::{IntoListComponent, is_list_component_value};
use crate::vcard::{group_from_name, split_list};
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the components of the delivery address for the vCard object.
#[derive(Clone, Debug, Default)]
pub struct Address {
    pub post_office_box: IntoListComponent,
    /// E.g apartment, suite, unit, building, floor, etc
    pub extended_address: IntoListComponent,
    pub street: IntoListComponent,
    /// AKA City
    pub locality: IntoListComponent,
    /// State or Province
    pub region: IntoListComponent,
    pub postal_code: IntoListComponent,
    pub country: IntoListComponent,

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

impl Address {
    /// Create a new ADR property without any parameter or group
    #[must_use]
    pub fn new(
        post_office_box: String,
        extended_address: String,
        street: String,
        locality: String,
        region: String,
        postal_code: String,
        country: String,
    ) -> Self {
        Self {
            post_office_box: post_office_box.into(),
            extended_address: extended_address.into(),
            street: street.into(),
            locality: locality.into(),
            region: region.into(),
            postal_code: postal_code.into(),
            country: country.into(),
            ..Default::default()
        }
    }
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
        let mut values: [String; 7] = split_list(&value, ';')
            .try_into()
            .map_err(|_| VCardError::InvalidValue(PropertyKind::Adr, value.clone()))?;

        let mut result = Self::new(
            mem::take(&mut values[0]),
            mem::take(&mut values[1]),
            mem::take(&mut values[2]),
            mem::take(&mut values[3]),
            mem::take(&mut values[4]),
            mem::take(&mut values[5]),
            mem::take(&mut values[6]),
        );

        result.group = group_from_name(property.name.as_str());
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

/// Validate that the given `property` respect the format for a `ADR` property
///
/// # Errors
///   * if property value is not a list of 7 `list_component` separated by semicolon
///   * if anu parameter is invalid
pub fn validate_adr(property: &IcalProperty) -> VcardValidationResult<()> {
    // ADR-param = "VALUE=text" / label-param / language-param / geo-parameter / tz-parameter / altid-param / pid-param / pref-param / type-param / any-param
    // ADR-value = ADR-component-pobox ";" ADR-component-ext ";" ADR-component-street ";" ADR-component-locality ";" ADR-component-region ";" ADR-component-code ";" ADR-component-country
    // ADR-component-pobox    = list-component
    // ADR-component-ext      = list-component
    // ADR-component-street   = list-component
    // ADR-component-locality = list-component
    // ADR-component-region   = list-component
    // ADR-component-code     = list-component
    // ADR-component-country  = list-component
    if let Some(value) = &property.value {
        if check_list(value, is_list_component_value, ';').is_some_and(|c| c == 7) {
            validate_parameters(
                property,
                ValueType::Text,
                &hash_set!(
                    ParameterType::Value,
                    ParameterType::Label,
                    ParameterType::Language,
                    ParameterType::Geo,
                    ParameterType::TZ,
                    ParameterType::AltId,
                    ParameterType::Pid,
                    ParameterType::Pref,
                    ParameterType::Type,
                    ParameterType::Any
                ),
            )?;
        } else {
            return Err(VcardValidationError::InvalidPropertyValue(
                get_property_kind(&property.name)?,
            ));
        }
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
