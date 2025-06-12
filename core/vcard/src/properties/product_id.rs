use std::collections::HashSet;
use std::fmt::{Debug, Display, Formatter};

use ical::generator::Property as IcalProperty;

use crate::parameters::any::Any;
use crate::parameters::preference::Preference;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the identifier for the product that created the vCard object.
#[derive(Clone, Debug, Default)]
pub struct ProductId {
    pub value: String,
    pub value_type: Option<ValueType>,
    pub any: HashSet<Any>,
    pub group: Option<String>,
    pub r#type: HashSet<GenericType>,
}

impl Display for ProductId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl TryFrom<IcalProperty> for ProductId {
    type Error = VCardError;

    fn try_from(property: IcalProperty) -> VCardResult<Self> {
        let value = property.value.expect("Missing value");

        let mut result = Self {
            value,
            ..Default::default()
        };

        result.group = group_from_name(&property.name);
        if let Some(parameters) = property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::ProdId))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::ProdId))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Role))?;
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::ProdId,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for ProductId {
    fn get_preference(&self) -> Option<Preference> {
        None
    }
}
