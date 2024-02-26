use serde::{Deserialize, Deserializer};
use std::fmt::{Display, Formatter};

/// Represent an user's API key ID.
#[derive(Debug, Deserialize, Eq, PartialEq, Hash, Clone)]
pub struct KeyId(String);

impl Display for KeyId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Into<String>> From<T> for KeyId {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl AsRef<str> for KeyId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct LockedKey {
    #[serde(rename = "ID")]
    pub id: KeyId,
    pub private_key: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub signature: String,
    #[serde(deserialize_with = "bool_from_integer")]
    pub primary: bool,
    #[serde(deserialize_with = "bool_from_integer")]
    pub active: bool,
    pub flags: Option<u32>,
}

/// Deserialize bool from integer
fn bool_from_integer<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    if i64::deserialize(deserializer)? == 0_i64 {
        Ok(false)
    } else {
        Ok(true)
    }
}
