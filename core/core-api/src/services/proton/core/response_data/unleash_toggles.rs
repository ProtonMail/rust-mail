use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize, Default))]
#[serde(rename_all = "camelCase")]
pub struct UnleashToggle {
    pub name: String,
    /// According to the Unleash API, always true.
    /// <https://docs.getunleash.io/reference/api/unleash/get-frontend-features/>
    ///
    /// See: [`UnleashToggleVariant::feature_enabled`]
    pub enabled: bool,
    /// `true` if the impression data collection is enabled for the feature.
    pub impression_data: bool,
    pub variant: UnleashToggleVariant,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize, Default))]
// Yes, Unleash API is inconsistent in naming fields
#[serde(rename_all = "snake_case")]
pub struct UnleashToggleVariant {
    pub name: String,
    #[serde(default = "default_feature_enabled")]
    pub feature_enabled: bool,
    #[serde(default)]
    pub payload: Option<UnleashTogglePayload>,
}

fn default_feature_enabled() -> bool {
    // Educated guess: If `feature_enabled` is missing, it means the feature is enabled.
    true
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize, Default))]
pub struct UnleashTogglePayload {
    #[serde(rename = "type")]
    pub ty: UnleashTogglePayloadType,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize, Default))]
#[serde(rename_all = "snake_case")]
pub enum UnleashTogglePayloadType {
    Json,
    Csv,
    #[cfg_attr(feature = "mocks", default)]
    String,
    Number,
}
