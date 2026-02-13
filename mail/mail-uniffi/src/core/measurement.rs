use proton_core_common::datatypes::{
    MeasurementEventType as CoreMeasurementEventType, MeasurementValue as CoreMeasurementValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MeasurementEventType {
    Install,
    Signup,
    Sub,
    FeatureUsage,
    Uninstall,
    Open { new_session: bool },
    OptOut,
}

impl From<MeasurementEventType> for CoreMeasurementEventType {
    fn from(value: MeasurementEventType) -> Self {
        match value {
            MeasurementEventType::Install => Self::Install,
            MeasurementEventType::Signup => Self::Signup,
            MeasurementEventType::Sub => Self::Sub,
            MeasurementEventType::FeatureUsage => Self::FeatureUsage,
            MeasurementEventType::Uninstall => Self::Uninstall,
            MeasurementEventType::Open { .. } => Self::Open,
            MeasurementEventType::OptOut => Self::OptOut,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MeasurementValue {
    String { value: String },
    Bool { value: bool },
}

impl From<MeasurementValue> for CoreMeasurementValue {
    fn from(value: MeasurementValue) -> Self {
        match value {
            MeasurementValue::String { value } => Self::String(value),
            MeasurementValue::Bool { value } => Self::Bool(value),
        }
    }
}
