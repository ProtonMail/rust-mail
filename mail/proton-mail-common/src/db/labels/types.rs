use proton_api_mail::exports::serde::{self, Deserialize};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(crate = "self::serde")]
pub struct LabelColor(String);

#[cfg(feature = "uniffi")]
uniffi::custom_newtype!(LabelColor, String);

impl LabelColor {
    pub fn purple() -> Self {
        Self("#8080FF".into())
    }

    pub fn black() -> Self {
        Self("#000000".into())
    }
}
