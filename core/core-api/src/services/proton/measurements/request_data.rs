use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum MeasurementEventType {
    Install,
    Signup,
    Sub,
    FeatureUsage,
    Uninstall,
    Open,
    OptOut,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum MeasurementValue {
    String(String),
    Bool(bool),
}
