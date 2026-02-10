use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PostMetricsRequestElement {
    pub name: String,
    pub version: u64,
    pub timestamp: i64,
    pub data: PostMetricsRequestData,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PostMetricsRequestData {
    pub labels: serde_json::Value,
    pub value: u64,
}
