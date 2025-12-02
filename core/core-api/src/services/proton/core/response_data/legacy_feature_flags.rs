//! Link to API docs: <https://protonmail.gitlab-pages.protontech.ch/Slim-API/core/#tag/Feature/operation/get_core-v4-features>

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

    // Even though Proton API docs (state from 12.2025) say that those
    // fields are required - in practice we are getting responses without them.
    #[serde(default)]
    pub global: bool,

    #[serde(default)]
    pub writable: bool,

    #[serde(default)]
    pub expiration_time: Option<u64>,

    #[serde(default)]
    pub update_time: Option<u64>,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LegacyFeatureFlagType {
    Boolean,
    Integer,
    Float,
    String,
    Enumeration,
    Mixed,
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
