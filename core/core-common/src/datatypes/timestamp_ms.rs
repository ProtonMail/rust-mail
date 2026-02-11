use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnixTimestampMs(u128);

impl UnixTimestampMs {
    #[must_use]
    pub fn new(value: u128) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn now() -> Self {
        Self(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before Unix epoch")
                .as_millis(),
        )
    }

    #[must_use]
    pub fn as_u128(&self) -> u128 {
        self.0
    }
}

impl From<u128> for UnixTimestampMs {
    fn from(value: u128) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for UnixTimestampMs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
