use std::collections::HashMap;

use derive_more::derive::TryFrom;
use proton_core_api::services::proton::{
    MeasurementEventType as ApiMeasurementEventType, MeasurementValue as ApiMeasurementValue,
};
use serde::{Deserialize, Serialize};
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::utils::sql_using_serde;

use super::UnixTimestampMs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalMeasurementId(u64);
impl From<u64> for LocalMeasurementId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl FromSql for LocalMeasurementId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        u64::column_result(value).map(LocalMeasurementId)
    }
}

impl ToSql for LocalMeasurementId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum MeasurementEventType {
    Install = 0,
    Signup = 1,
    Sub = 2,
    FeatureUsage = 3,
    Uninstall = 4,
    Open = 5,
    OptOut = 6,
}

impl From<ApiMeasurementEventType> for MeasurementEventType {
    fn from(value: ApiMeasurementEventType) -> Self {
        match value {
            ApiMeasurementEventType::Install => Self::Install,
            ApiMeasurementEventType::Signup => Self::Signup,
            ApiMeasurementEventType::Sub => Self::Sub,
            ApiMeasurementEventType::FeatureUsage => Self::FeatureUsage,
            ApiMeasurementEventType::Uninstall => Self::Uninstall,
            ApiMeasurementEventType::Open => Self::Open,
            ApiMeasurementEventType::OptOut => Self::OptOut,
        }
    }
}

impl From<MeasurementEventType> for ApiMeasurementEventType {
    fn from(value: MeasurementEventType) -> Self {
        match value {
            MeasurementEventType::Install => Self::Install,
            MeasurementEventType::Signup => Self::Signup,
            MeasurementEventType::Sub => Self::Sub,
            MeasurementEventType::FeatureUsage => Self::FeatureUsage,
            MeasurementEventType::Uninstall => Self::Uninstall,
            MeasurementEventType::Open => Self::Open,
            MeasurementEventType::OptOut => Self::OptOut,
        }
    }
}

impl FromSql for MeasurementEventType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for MeasurementEventType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MeasurementValue {
    String(String),
    Bool(bool),
}

impl From<ApiMeasurementValue> for MeasurementValue {
    fn from(value: ApiMeasurementValue) -> Self {
        match value {
            ApiMeasurementValue::String(s) => Self::String(s),
            ApiMeasurementValue::Bool(b) => Self::Bool(b),
        }
    }
}

impl From<MeasurementValue> for ApiMeasurementValue {
    fn from(value: MeasurementValue) -> Self {
        match value {
            MeasurementValue::String(s) => Self::String(s),
            MeasurementValue::Bool(b) => Self::Bool(b),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementData {
    pub event_type: MeasurementEventType,
    pub event_timestamp_ms: UnixTimestampMs,
    pub asid: String,
    pub app_package_name: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, Option<MeasurementValue>>,
}

sql_using_serde!(MeasurementData);
