use crate::db::DBResult;
use proton_api_core::exports::serde::de::DeserializeOwned;
use proton_api_core::exports::serde_json;
use proton_sqlite3::rusqlite::Row;
use std::str;

pub fn serde_json_err_to_sql_err(v: serde_json::Error) -> proton_sqlite3::rusqlite::Error {
    proton_sqlite3::rusqlite::Error::ToSqlConversionFailure(Box::new(v))
}

#[allow(clippy::module_name_repetitions)]
pub fn deserialize_json<T: DeserializeOwned>(value: &str) -> DBResult<T> {
    serde_json::from_str(value).map_err(serde_json_err_to_sql_err)
}

pub fn deserialize_json_from_row<T: DeserializeOwned>(r: &Row, index: usize) -> DBResult<T> {
    let value_ref = r.get_ref(index)?;
    let str = value_ref.as_str()?;
    deserialize_json(str)
}
