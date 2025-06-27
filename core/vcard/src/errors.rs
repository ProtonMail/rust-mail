use crate::{ParameterType, PropertyKind, ValueType};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VCardError {
    #[error("In a property {0:?}, invalid parameter name: {1:?}")]
    InvalidParameterName(PropertyKind, String),
    #[error("In a property {0:?}, a parameter {1:?} have an invalid value: {2:?}")]
    InvalidParameterValue(PropertyKind, ParameterType, String),
    #[error("In a property {0:?}, a parameter {1:?} have invalid values: {2:?}")]
    InvalidParameterValues(PropertyKind, ParameterType, Vec<String>),
    #[error("In a property {0:?}, a parameter {1:?} have an invalid value of type({2:?}): {3:?}")]
    InvalidParameterValueWithType(PropertyKind, ParameterType, ValueType, String),
    #[error("Invalid property name: {0:?}")]
    InvalidPropertyName(String),
    #[error("Invalid value for property {0:?}: {1:?}")]
    InvalidValue(PropertyKind, String),
    #[error("Invalid parameter value for {0:?}: {2:?} expected a {1:?}")]
    InvalidValueWithType(PropertyKind, ValueType, String),
    #[error("No property VERSION a vCard beginning")]
    MissingVersion,
    #[error("Property {0:?} is missing a value")]
    MissingValue(PropertyKind),
    #[error("In a property {0:?}, parameter {1:?} expected exactly one element: {2:?}")]
    ParameterExpectedExactlyOneValue(PropertyKind, ParameterType, Vec<String>),
    #[error("In a property {0:?}, parameter {1:?} expected at least one element")]
    ParameterExpectedAtLeastOneValue(PropertyKind, ParameterType),
    #[error("Unexpected parameter for {0:?}: {1:?}")]
    UnexpectedParameter(PropertyKind, ParameterType),
    #[error("Unexpected error: {0:?}")]
    Unexpected(#[from] anyhow::Error),
}

pub type VCardResult<T> = Result<T, VCardError>;

impl VCardError {
    pub(crate) fn from_value_error(
        property_kind: PropertyKind,
    ) -> impl Fn(VCardValueError) -> Self {
        move |error| match error {
            VCardValueError::Invalid(value_type, value) => {
                Self::InvalidValueWithType(property_kind.clone(), value_type, value)
            }
        }
    }

    pub(crate) fn from_parameter_error(
        property_kind: PropertyKind,
    ) -> impl Fn(VCardParameterError) -> Self {
        move |error| match error {
            VCardParameterError::ExpectedExactlyOneValue(parameter_type, values) => {
                Self::ParameterExpectedExactlyOneValue(
                    property_kind.clone(),
                    parameter_type,
                    values,
                )
            }
            VCardParameterError::ExpectedAtLeastOneValue(parameter_type) => {
                Self::ParameterExpectedAtLeastOneValue(property_kind.clone(), parameter_type)
            }
            VCardParameterError::InvalidPropertyName(name) => Self::InvalidPropertyName(name),
            VCardParameterError::InvalidName(name) => {
                Self::InvalidParameterName(property_kind.clone(), name)
            }
            VCardParameterError::InvalidValue(parameter_type, value) => {
                Self::InvalidParameterValue(property_kind.clone(), parameter_type, value)
            }
            VCardParameterError::InvalidValues(parameter_type, values) => {
                Self::InvalidParameterValues(property_kind.clone(), parameter_type, values)
            }
            VCardParameterError::InvalidValueWithType(parameter_type, value_type, value) => {
                Self::InvalidParameterValueWithType(
                    property_kind.clone(),
                    parameter_type,
                    value_type,
                    value,
                )
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum VCardParameterError {
    #[error("Was expecting exactly one value for {0:?}: {1:?}")]
    ExpectedExactlyOneValue(ParameterType, Vec<String>),
    #[error("Was expecting at least one value for {0:?} got none")]
    ExpectedAtLeastOneValue(ParameterType),
    #[error("Invalid property name: {0:?} ")]
    InvalidPropertyName(String),
    #[error("Invalid parameter name: {0:?} ")]
    InvalidName(String),
    #[error("Invalid parameter value for {0:?}: {1:?} ")]
    InvalidValue(ParameterType, String),
    #[error("Invalid parameter values for {0:?}: {1:?} ")]
    InvalidValues(ParameterType, Vec<String>),
    #[error("Invalid parameter value for {0:?}: {2:?} expected a {1:?}")]
    InvalidValueWithType(ParameterType, ValueType, String),
}

pub type VCardParameterResult<T> = Result<T, VCardParameterError>;

impl VCardParameterError {
    pub(crate) fn from_value_error(
        parameter_type: ParameterType,
    ) -> impl Fn(VCardValueError) -> Self {
        move |error| match error {
            VCardValueError::Invalid(value_type, value) => {
                Self::InvalidValueWithType(parameter_type, value_type, value)
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum VCardValueError {
    #[error("Invalid value for {0:?}: {1:?}")]
    Invalid(ValueType, String),
}

pub type VCardValueResult<T> = Result<T, VCardValueError>;

#[derive(Debug, Error)]
pub enum VcardValidationError {
    #[error("Invalid property value: {0:?}")]
    InvalidPropertyValue(PropertyKind),
    #[error("Invalid property name: {0:?}")]
    InvalidPropertyName(String),
    #[error("Invalid property parameter: {0:?}:{1:?}")]
    InvalidPropertyParam(PropertyKind, String),
    #[error("Invalid properties order")]
    InvalidPropertiesOrder,
    #[error("Invalid properties group name: {0:?}")]
    InvalidPropertyGroupName(String),
    #[error("Unexpected param: {0:?}:{1:?}")]
    UnexpectedPropertyParam(PropertyKind, String),
    #[error("Unexpected parameter name: {0:?}")]
    UnexpectedPropertyParamName(String),
    #[error("ICal parser error: {0}")]
    ICalParserError(#[from] ical::parser::ParserError),
    #[error("URI error: {0}")]
    UriDecodeError(#[from] url::ParseError),
}

pub type VcardValidationResult<T> = Result<T, VcardValidationError>;
