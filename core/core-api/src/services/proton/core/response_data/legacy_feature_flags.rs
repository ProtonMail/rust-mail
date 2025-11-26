use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct LegacyFeatureFlag {
    #[serde(flatten)]
    pub metadata: LegacyFeatureFlagMetadata,

    #[serde(flatten)]
    pub variant: LegacyFeatureFlagVariant,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct LegacyFeatureFlagMetadata {
    pub code: String,

    pub global: bool,

    pub writable: bool,

    pub expiration_time: u64,

    pub update_time: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
#[must_use]
pub struct Value<T> {
    pub value: T,
    pub default_value: T,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
#[must_use]
pub struct RangedValue<T> {
    pub value: T,
    pub default_value: T,
    pub minimum: T,
    pub maximum: T,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "Type")]
pub enum LegacyFeatureFlagVariant {
    Boolean(Value<bool>),
    Integer(RangedValue<i32>),
    Float(RangedValue<f64>),
    String(Value<String>),
    Enumeration(Value<String>),
    Mixed(Value<serde_json::Value>),
}

impl LegacyFeatureFlagVariant {
    #[must_use]
    pub fn into_bool(self) -> Option<Value<bool>> {
        match self {
            Self::Boolean(b) => Some(b),
            _ => None,
        }
    }
}
